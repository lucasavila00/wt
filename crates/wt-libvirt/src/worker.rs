use crate::{LibvirtConfig, ProvisionSpec, WorkerError, World, WorldWorker};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;

pub struct LibvirtWorker {
    config: LibvirtConfig,
}

impl LibvirtWorker {
    pub fn new(config: LibvirtConfig) -> Result<Self, WorkerError> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|error| worker_error("KVM is required but /dev/kvm is unavailable", error))?;
        require_file(&config.image, "guest image")?;
        if !config.worlds_dir.is_dir() {
            return Err(WorkerError::new(format!(
                "worlds directory not found: {}",
                config.worlds_dir.display()
            )));
        }
        Connect::open(Some(&config.libvirt_uri))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        Ok(Self { config })
    }

    fn provision_inner(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
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

        fs::write(&paths.user_data, "#cloud-config\n")
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

        let connection = Connect::open(Some(&self.config.libvirt_uri))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let xml = domain_xml(
            spec.backend_id,
            &paths.disk,
            &paths.seed,
            &self.config.network,
            &self.config.architecture,
            &self.config.machine,
            self.config.memory_mib,
            self.config.vcpus,
        );
        let domain = Domain::define_xml(&connection, &xml)
            .map_err(|error| worker_error("define KVM domain", error))?;
        domain
            .create()
            .map_err(|error| worker_error("start KVM domain", error))?;

        self.wait_for_guest_agent(&domain)?;
        let guest_ip = self.wait_for_ip(spec.backend_id)?;
        Ok(World { guest_ip })
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
        let request = serde_json::json!({
            "execute": "guest-exec",
            "arguments": {
                "path": "/bin/sh",
                "arg": [
                    "-lc",
                    "docker info >/dev/null && docker compose version >/dev/null"
                ],
                "capture-output": true
            }
        });
        let response = domain
            .qemu_agent_command(&request.to_string(), 10, 0)
            .map_err(|error| worker_error("start guest readiness command", error))?;
        let response: serde_json::Value = serde_json::from_str(&response)
            .map_err(|error| worker_error("decode guest agent response", error))?;
        let pid = response["return"]["pid"]
            .as_u64()
            .ok_or_else(|| WorkerError::new("guest agent did not return an execution pid"))?;

        loop {
            let request = serde_json::json!({
                "execute": "guest-exec-status",
                "arguments": { "pid": pid }
            });
            let response = domain
                .qemu_agent_command(&request.to_string(), 10, 0)
                .map_err(|error| worker_error("read guest readiness command", error))?;
            let response: serde_json::Value = serde_json::from_str(&response)
                .map_err(|error| worker_error("decode guest agent response", error))?;
            let result = &response["return"];
            if result["exited"].as_bool() == Some(true) {
                return match result["exitcode"].as_i64() {
                    Some(0) => Ok(()),
                    exit_code => Err(WorkerError::new(format!(
                        "Docker or Compose readiness check failed with exit code {exit_code:?}"
                    ))),
                };
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(
                    "timed out waiting for Docker and Compose readiness",
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
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
                (ip.is_ipv4() && !ip.is_loopback()).then(|| ip.to_string())
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
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        match self.provision_inner(spec) {
            Ok(world) => Ok(world),
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

    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError> {
        Ok(self
            .domain_ip(backend_id)?
            .map(|guest_ip| World { guest_ip }))
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

fn domain_xml(
    name: &str,
    disk: &Path,
    seed: &Path,
    network: &str,
    architecture: &str,
    machine: &str,
    memory_mib: u64,
    vcpus: u32,
) -> String {
    let disk = disk.to_string_lossy();
    let seed = seed.to_string_lossy();
    let name = quick_xml::escape::escape(name);
    let disk = quick_xml::escape::escape(disk.as_ref());
    let seed = quick_xml::escape::escape(seed.as_ref());
    let network = quick_xml::escape::escape(network);
    let architecture = quick_xml::escape::escape(architecture);
    let machine = quick_xml::escape::escape(machine);
    format!(
        "<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory_mib}</memory>
  <vcpu>{vcpus}</vcpu>
  <os firmware='efi'>
    <type arch='{architecture}' machine='{machine}'>hvm</type>
    <firmware><feature enabled='no' name='secure-boot'/></firmware>
  </os>
  <features><acpi/><apic/></features>
  <cpu mode='host-passthrough' check='none'/>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='{seed}'/>
      <target dev='sda' bus='sata'/>
      <readonly/>
    </disk>
    <interface type='network'>
      <source network='{network}'/>
      <model type='virtio'/>
    </interface>
    <channel type='unix'>
      <target type='virtio' name='org.qemu.guest_agent.0'/>
    </channel>
    <serial type='pty'><target port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
    <rng model='virtio'><backend model='random'>/dev/urandom</backend></rng>
  </devices>
</domain>"
    )
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
