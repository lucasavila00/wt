//! Production orchestration for one libvirt/KVM world.

mod devcontainer;
mod git;
mod guest_agent;
mod world;

use crate::{LibvirtConfig, ProvisionSpec, WorkerError, World, WorldWorker};
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;

pub struct LibvirtWorker {
    config: LibvirtConfig,
    app_shell: Vec<u8>,
}

impl LibvirtWorker {
    pub fn new(config: LibvirtConfig) -> Result<Self, WorkerError> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|error| worker_error("KVM is required but /dev/kvm is unavailable", error))?;
        require_file(&config.image, "guest image")?;
        require_file(&config.app_shell_binary, "guest app-shell binary")?;
        let app_shell = fs::read(&config.app_shell_binary)
            .map_err(|error| worker_error("read guest app-shell binary", error))?;
        if !config.worlds_dir.is_dir() {
            return Err(WorkerError::new(format!(
                "worlds directory not found: {}",
                config.worlds_dir.display()
            )));
        }
        Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        Ok(Self { config, app_shell })
    }

    fn provision_inner(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        wt_api::validate_ssh_git_source(spec.source)
            .map_err(|error| WorkerError::new(format!("Git source: {error}")))?;
        let private_git = git::load_credentials(spec.identity_file)?;
        eprintln!("Creating KVM guest {}...", spec.name);
        let paths = world::Paths::new(&self.config.worlds_dir, spec.backend_id);
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

        fs::write(
            &paths.user_data,
            world::cloud_config(&self.config.ssh_authorized_keys),
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

        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let xml = world::domain_xml(spec.backend_id, &paths, &self.config);
        let domain = Domain::define_xml(&connection, &xml)
            .map_err(|error| worker_error("define KVM domain", error))?;
        domain
            .create()
            .map_err(|error| worker_error("start KVM domain", error))?;

        // QEMU guest-agent is the provisioning channel. SSH is exposed to the
        // user, but wt does not depend on it to configure the world.
        eprintln!("Waiting for the guest agent...");
        let phase_started = Instant::now();
        self.wait_for_guest_agent(&domain)?;
        report_phase("guest agent and Docker readiness", phase_started);
        eprintln!("Waiting for guest networking...");
        let phase_started = Instant::now();
        let guest_ip = self.wait_for_ip(spec.backend_id)?;
        report_phase("guest networking", phase_started);
        let recipe_deadline = Instant::now() + self.config.recipe_timeout;
        eprintln!("Waiting for guest SSH...");
        let phase_started = Instant::now();
        self.wait_for_ssh(&guest_ip, recipe_deadline)?;
        let host_keys = self.read_host_keys(&domain, recipe_deadline)?;
        report_phase("guest SSH readiness", phase_started);
        eprintln!("Cloning {}...", spec.source);
        let phase_started = Instant::now();
        git::clone_and_checkout(
            &domain,
            spec.source,
            spec.git_ref,
            &private_git,
            recipe_deadline,
        )?;
        report_phase("Git clone and checkout", phase_started);
        eprintln!("Starting the repository devcontainer...");
        let phase_started = Instant::now();
        guest_agent::run_phase(
            &domain,
            "workspace ownership",
            "/bin/chown",
            &["-R", "wt:wt", "/workspace"],
            recipe_deadline,
        )?;
        guest_agent::run_phase(
            &domain,
            "devcontainer up",
            "/usr/sbin/runuser",
            &[
                "-u",
                "wt",
                "--",
                "/usr/bin/env",
                "HOME=/home/wt",
                "/usr/local/bin/devcontainer",
                "up",
                "--workspace-folder",
                "/workspace",
            ],
            recipe_deadline,
        )?;
        report_phase("devcontainer up", phase_started);
        let phase_started = Instant::now();
        devcontainer::install_app_shell(&domain, &self.app_shell, recipe_deadline)?;
        guest_agent::run_phase(
            &domain,
            "devcontainer Git credentials",
            "/usr/local/bin/devcontainer",
            &[
                "exec",
                "--workspace-folder",
                "/workspace",
                "/bin/sh",
                "-c",
                "workspace=$(pwd -P) && git config --global --add safe.directory \"$workspace\" && directory=$(git rev-parse --git-common-dir)/wt && test -r \"$directory/identity\" && test -x \"$directory/ssh\" && test -r \"$directory/known_hosts\" && test -n \"$(git config --get core.sshCommand)\"",
            ],
            recipe_deadline,
        )?;
        report_phase("app shell and Git credential verification", phase_started);
        eprintln!("World {} is ready.", spec.name);
        Ok(World {
            guest_ip: guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: guest_ip,
                port: 22,
                host_keys,
            },
        })
    }

    fn wait_for_ssh(&self, guest_ip: &str, deadline: Instant) -> Result<(), WorkerError> {
        let address: SocketAddr = format!("{guest_ip}:22")
            .parse()
            .map_err(|error| worker_error("parse guest SSH address", error))?;
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(
                    "SSH readiness: timed out waiting for port 22",
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn read_host_keys(
        &self,
        domain: &Domain,
        deadline: Instant,
    ) -> Result<Vec<String>, WorkerError> {
        let output = guest_agent::capture_phase(
            domain,
            "SSH host keys",
            "/bin/sh",
            &["-c", "cat /etc/ssh/ssh_host_*_key.pub"],
            deadline,
        )?;
        let keys = String::from_utf8_lossy(&output)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return Err(WorkerError::new(
                "SSH host keys: guest returned no public host keys",
            ));
        }
        Ok(keys)
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

    fn wait_for_guest_agent(&self, domain: &Domain) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            if domain
                .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                .is_ok()
            {
                return self.verify_guest_tools(domain, deadline);
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new("timed out waiting for QEMU guest agent"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn verify_guest_tools(&self, domain: &Domain, deadline: Instant) -> Result<(), WorkerError> {
        guest_agent::run_phase(
            domain,
            "Docker and Compose readiness",
            "/bin/sh",
            &[
                "-lc",
                "docker info >/dev/null && docker compose version >/dev/null",
            ],
            deadline,
        )?;
        Ok(())
    }

    fn domain_ip(&self, backend_id: &str) -> Result<Option<String>, WorkerError> {
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
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
                (ip.is_ipv4() && !ip.is_loopback()).then(|| ip.to_string())
            }))
    }

    fn remove_domain(&self, backend_id: &str) -> Result<(), WorkerError> {
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
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
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        match self.provision_inner(spec) {
            Ok(world) => Ok(world),
            Err(error) => {
                // A failed create must not leave a domain or overlay behind.
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

    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError> {
        let Some(guest_ip) = self.domain_ip(backend_id)? else {
            return Ok(None);
        };
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = Domain::lookup_by_name(&connection, backend_id)
            .map_err(|error| worker_error("look up libvirt domain", error))?;
        // DHCP addresses can move; the per-world host keys are the stable SSH identity.
        let host_keys = self.read_host_keys(&domain, Instant::now() + self.config.boot_timeout)?;
        Ok(Some(World {
            guest_ip: guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: guest_ip,
                port: 22,
                host_keys,
            },
        }))
    }
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

fn report_phase(label: &str, started: Instant) {
    eprintln!("{label} ready in {:.1}s.", started.elapsed().as_secs_f64());
}
