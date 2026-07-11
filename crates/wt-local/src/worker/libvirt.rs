use crate::config::LocalConfig;
use crate::worker::{ProvisionSpec, WorkerError, WorldWorker};
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;
use wt_api::SshEndpoint;

pub struct LibvirtWorker {
    config: LocalConfig,
}

impl LibvirtWorker {
    pub fn new(config: LocalConfig) -> Result<Self, WorkerError> {
        require_file(&config.image, "guest image")?;
        require_file(&config.ssh_public_key, "SSH public key")?;
        require_file(&config.ssh_private_key, "SSH private key")?;
        fs::create_dir_all(&config.worlds_dir)
            .map_err(|error| worker_error("create worlds directory", error))?;
        Connect::open(Some(&config.libvirt_uri))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        Ok(Self { config })
    }

    fn provision_inner(&self, spec: &ProvisionSpec<'_>) -> Result<SshEndpoint, WorkerError> {
        let paths = WorldPaths::new(&self.config.worlds_dir, spec.backend_id);
        fs::create_dir(&paths.directory)
            .map_err(|error| worker_error("create world directory", error))?;

        run(
            Command::new("qemu-img")
                .args(["create", "-q", "-f", "qcow2", "-F", "qcow2", "-b"])
                .arg(&self.config.image)
                .arg(&paths.disk)
                .arg(format!("{}G", self.config.disk_gib)),
            "create qcow2 overlay",
        )?;

        let public_key = read_public_key(&self.config.ssh_public_key)?;
        fs::write(
            &paths.user_data,
            user_data(&self.config.guest_user, &public_key),
        )
        .map_err(|error| worker_error("write cloud-init user-data", error))?;
        fs::write(
            &paths.meta_data,
            format!(
                "instance-id: {}\nlocal-hostname: {}\n",
                spec.backend_id, spec.name
            ),
        )
        .map_err(|error| worker_error("write cloud-init meta-data", error))?;
        run(
            Command::new("cloud-localds")
                .arg(&paths.seed)
                .arg(&paths.user_data)
                .arg(&paths.meta_data),
            "create cloud-init seed",
        )?;

        let virt_type = if Path::new("/dev/kvm").exists() {
            "kvm"
        } else {
            "qemu"
        };
        run(
            Command::new("virt-install")
                .args(["--connect", &self.config.libvirt_uri])
                .args(["--name", spec.backend_id])
                .args(["--memory", &self.config.memory_mib.to_string()])
                .args(["--vcpus", &self.config.vcpus.to_string()])
                .args(["--virt-type", virt_type])
                .args(["--os-variant", "ubuntu24.04"])
                .args(["--import", "--boot", "uefi"])
                .arg("--disk")
                .arg(format!(
                    "path={},format=qcow2,bus=virtio",
                    paths.disk.display()
                ))
                .arg("--disk")
                .arg(format!("path={},device=cdrom", paths.seed.display()))
                .arg("--network")
                .arg(format!("network={},model=virtio", self.config.network))
                .args(["--graphics", "none", "--noautoconsole", "--wait", "0"]),
            "define and start libvirt domain",
        )?;

        let host = self.wait_for_ip(spec.backend_id)?;
        self.wait_for_guest(&host)?;
        Ok(SshEndpoint {
            user: self.config.guest_user.clone(),
            host,
            port: 22,
        })
    }

    fn wait_for_ip(&self, backend_id: &str) -> Result<String, WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            if let Some(host) = self.domain_ip(backend_id)? {
                return Ok(host);
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for IP for domain {backend_id}"
                )));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn wait_for_guest(&self, host: &str) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        let address = SocketAddr::new(
            host.parse::<IpAddr>()
                .map_err(|error| worker_error("parse guest IP", error))?,
            22,
        );
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
                let output = Command::new("ssh")
                    .args(["-i"])
                    .arg(&self.config.ssh_private_key)
                    .args([
                        "-o",
                        "BatchMode=yes",
                        "-o",
                        "StrictHostKeyChecking=no",
                        "-o",
                        "UserKnownHostsFile=/dev/null",
                        "-o",
                        "ConnectTimeout=5",
                    ])
                    .arg(format!("{}@{host}", self.config.guest_user))
                    .arg("cloud-init status --wait && sudo docker info >/dev/null")
                    .output();
                if output.as_ref().is_ok_and(Output::status_success) {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for Docker-ready SSH guest at {host}"
                )));
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    }

    fn domain_ip(&self, backend_id: &str) -> Result<Option<String>, WorkerError> {
        let connection = Connect::open(Some(&self.config.libvirt_uri))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = match Domain::lookup_by_name(&connection, backend_id) {
            Ok(domain) => domain,
            Err(error) if error.code() == ErrorNumber::NoDomain => return Ok(None),
            Err(error) => return Err(worker_error("look up libvirt domain", error)),
        };
        let interfaces = domain
            .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
            .map_err(|error| worker_error("get domain interface addresses", error))?;
        Ok(interfaces
            .into_iter()
            .flat_map(|interface| interface.addrs)
            .find_map(|address| {
                let ip = address.addr.parse::<IpAddr>().ok()?;
                ip.is_ipv4().then(|| ip.to_string())
            }))
    }

    fn remove_domain(&self, backend_id: &str) -> Result<(), WorkerError> {
        let connection = Connect::open(Some(&self.config.libvirt_uri))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = match Domain::lookup_by_name(&connection, backend_id) {
            Ok(domain) => domain,
            Err(error) if error.code() == ErrorNumber::NoDomain => return Ok(()),
            Err(error) => return Err(worker_error("look up libvirt domain", error)),
        };
        if domain
            .is_active()
            .map_err(|error| worker_error("check domain state", error))?
        {
            domain
                .destroy()
                .map_err(|error| worker_error("destroy domain", error))?;
        }
        domain
            .undefine_flags(virt::sys::VIR_DOMAIN_UNDEFINE_NVRAM)
            .map_err(|error| worker_error("undefine domain", error))?;
        Ok(())
    }

    fn remove_files(&self, backend_id: &str) -> Result<(), WorkerError> {
        let directory = self.config.worlds_dir.join(backend_id);
        if directory.exists() {
            fs::remove_dir_all(&directory)
                .map_err(|error| worker_error("remove world files", error))?;
        }
        Ok(())
    }
}

impl WorldWorker for LibvirtWorker {
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<SshEndpoint, WorkerError> {
        match self.provision_inner(spec) {
            Ok(endpoint) => Ok(endpoint),
            Err(error) => {
                let _ = self.remove_domain(spec.backend_id);
                let _ = self.remove_files(spec.backend_id);
                Err(error)
            }
        }
    }

    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError> {
        self.remove_domain(backend_id)?;
        self.remove_files(backend_id)
    }

    fn inspect(&self, backend_id: &str) -> Result<Option<SshEndpoint>, WorkerError> {
        Ok(self.domain_ip(backend_id)?.map(|host| SshEndpoint {
            user: self.config.guest_user.clone(),
            host,
            port: 22,
        }))
    }
}

struct WorldPaths {
    directory: PathBuf,
    disk: PathBuf,
    seed: PathBuf,
    user_data: PathBuf,
    meta_data: PathBuf,
}

impl WorldPaths {
    fn new(root: &Path, backend_id: &str) -> Self {
        let directory = root.join(backend_id);
        Self {
            disk: directory.join("disk.qcow2"),
            seed: directory.join("seed.img"),
            user_data: directory.join("user-data"),
            meta_data: directory.join("meta-data"),
            directory,
        }
    }
}

fn user_data(user: &str, public_key: &str) -> String {
    format!(
        "#cloud-config\n\
         users:\n\
           - name: {user}\n\
             groups: [adm, sudo, docker]\n\
             sudo: ALL=(ALL) NOPASSWD:ALL\n\
             shell: /bin/bash\n\
             lock_passwd: true\n\
             ssh_authorized_keys:\n\
               - {public_key}\n\
         ssh_pwauth: false\n\
         package_update: true\n\
         packages:\n\
           - docker.io\n\
         runcmd:\n\
           - systemctl enable --now docker\n"
    )
}

fn read_public_key(path: &Path) -> Result<String, WorkerError> {
    let contents = fs::read_to_string(path).map_err(|error| worker_error("read SSH key", error))?;
    let key = contents.trim();
    if key.is_empty() || key.contains('\n') || !key.starts_with("ssh-") {
        return Err(WorkerError::new(format!(
            "invalid SSH public key {}",
            path.display()
        )));
    }
    Ok(key.to_owned())
}

fn require_file(path: &Path, label: &str) -> Result<(), WorkerError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "{label} not found: {}",
            path.display()
        )))
    }
}

fn run(command: &mut Command, action: &str) -> Result<(), WorkerError> {
    let output = command
        .output()
        .map_err(|error| worker_error(action, error))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(WorkerError::new(format!("{action}: {stderr}")))
}

fn worker_error(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

trait OutputStatus {
    fn status_success(&self) -> bool;
}

impl OutputStatus for Output {
    fn status_success(&self) -> bool {
        self.status.success()
    }
}
