//! Libvirt/KVM machine lifecycle.

mod guest_agent;
mod world;

use crate::{MachineConfig, LIBVIRT_URI};
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;
use virt::network::Network;
use wt_command::cmd;
use wt_provider::{Machine, MachineProvider, MachineSpec, ProviderId, WorkerError};

struct LibvirtConnection(Connect);

impl LibvirtConnection {
    fn open() -> Result<Self, WorkerError> {
        Connect::open(Some(LIBVIRT_URI))
            .map(Self)
            .map_err(|error| context("connect to libvirt", error))
    }
}

impl Deref for LibvirtConnection {
    type Target = Connect;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for LibvirtConnection {
    fn drop(&mut self) {
        let _ = self.0.close();
    }
}

#[derive(Clone)]
pub struct LibvirtProvider {
    config: MachineConfig,
}

impl LibvirtProvider {
    pub fn new(config: MachineConfig) -> Result<Self, WorkerError> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|error| context("KVM is required but /dev/kvm is unavailable", error))?;
        require_file(&config.image, "guest image")?;
        if !config.worlds_dir.is_dir() {
            return Err(WorkerError::new(format!(
                "worlds directory not found: {}",
                config.worlds_dir.display()
            )));
        }
        let connection = LibvirtConnection::open()?;
        Network::lookup_by_name(&connection, &config.network)
            .map_err(|error| context("look up libvirt network", error))?;
        Ok(Self { config })
    }

    pub fn network_bridge_address(&self) -> Result<String, WorkerError> {
        let connection = LibvirtConnection::open()?;
        network_address(&connection, &self.config.network)
    }

    fn wait_for_agent(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            let domain = lookup_domain(provider_id)?;
            if domain
                .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                .is_ok()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new("timed out waiting for QEMU guest agent"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn wait_for_ip(&self, provider_id: &ProviderId) -> Result<String, WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            if let Some(ip) = domain_ip(provider_id)? {
                return Ok(ip);
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for IP for domain {provider_id}"
                )));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn machine(&self, provider_id: &ProviderId, guest_ip: String) -> Machine {
        Machine {
            provider_id: provider_id.clone(),
            guest_ip,
            transport: Arc::new(guest_agent::QemuGuestTransport::new(provider_id.clone())),
        }
    }

    fn remove_domain(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        let connection = LibvirtConnection::open()?;
        let domain = match Domain::lookup_by_name(&connection, provider_id.as_str()) {
            Ok(domain) => domain,
            Err(error) if error.code() == ErrorNumber::NoDomain => return Ok(()),
            Err(error) => return Err(context("look up libvirt domain", error)),
        };
        if domain
            .is_active()
            .map_err(|error| context("check domain state", error))?
        {
            domain
                .destroy()
                .map_err(|error| context("destroy domain", error))?;
        }
        domain
            .undefine_flags(virt::sys::VIR_DOMAIN_UNDEFINE_NVRAM)
            .map_err(|error| context("undefine domain", error))
    }

    fn remove_files(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        let directory = self.config.worlds_dir.join(provider_id.as_str());
        match fs::remove_dir_all(&directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(context("remove machine files", error)),
        }
    }

    fn cleanup(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        let mut errors = Vec::new();
        if let Err(error) = self.remove_domain(provider_id) {
            errors.push(error.to_string());
        }
        if let Err(error) = self.remove_files(provider_id) {
            errors.push(error.to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(WorkerError::new(format!(
                "delete libvirt machine: {}",
                errors.join("; ")
            )))
        }
    }

    fn create_inner(
        &self,
        spec: &MachineSpec,
        progress: &mut dyn Write,
    ) -> Result<Machine, WorkerError> {
        if spec.memory_mib == 0 || spec.vcpus == 0 || spec.disk_gib == 0 {
            return Err(WorkerError::new(
                "machine CPU, memory, and disk resources must be greater than zero",
            ));
        }
        writeln!(progress, "Creating KVM guest {}...", spec.provider_id)
            .map_err(|error| context("write machine progress", error))?;
        let paths = world::Paths::new(&self.config.worlds_dir, &spec.provider_id);
        fs::create_dir(&paths.directory)
            .map_err(|error| context("create machine directory", error))?;
        run(
            cmd!(
                "qemu-img",
                "create",
                "-q",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                &self.config.image,
                &paths.disk,
                format!("{}G", spec.disk_gib),
            ),
            "create qcow2 overlay",
        )?;
        fs::write(&paths.user_data, world::cloud_config())
            .map_err(|error| context("write cloud-init user-data", error))?;
        fs::write(
            &paths.meta_data,
            format!(
                "instance-id: {}\nlocal-hostname: {}\n",
                spec.provider_id, spec.provider_id
            ),
        )
        .map_err(|error| context("write cloud-init meta-data", error))?;
        fs::write(&paths.network_config, world::network_config())
            .map_err(|error| context("write cloud-init network-config", error))?;
        run(
            cmd!(
                "cloud-localds",
                "--network-config",
                &paths.network_config,
                &paths.seed,
                &paths.user_data,
                &paths.meta_data
            ),
            "create cloud-init seed",
        )?;
        prepare_qemu_file_access(&paths)?;
        let xml = world::domain_xml(&spec.provider_id, &paths, &self.config, spec);
        {
            let connection = LibvirtConnection::open()?;
            let domain = Domain::define_xml(&connection, &xml)
                .map_err(|error| context("define KVM domain", error))?;
            domain
                .create()
                .map_err(|error| context("start KVM domain", error))?;
        }
        writeln!(progress, "Waiting for the guest transport...")
            .map_err(|error| context("write machine progress", error))?;
        self.wait_for_agent(&spec.provider_id)?;
        let guest_ip = self.wait_for_ip(&spec.provider_id)?;
        writeln!(progress, "Machine transport ready at {guest_ip}.")
            .map_err(|error| context("write machine progress", error))?;
        Ok(self.machine(&spec.provider_id, guest_ip))
    }
}

impl MachineProvider for LibvirtProvider {
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError> {
        match self.create_inner(spec, progress) {
            Ok(machine) => Ok(machine),
            Err(primary) => {
                if let Err(cleanup) = self.cleanup(&spec.provider_id) {
                    Err(WorkerError::new(format!(
                        "{primary} (cleanup also failed: {cleanup})"
                    )))
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn inspect(&self, provider_id: &ProviderId) -> Result<Option<Machine>, WorkerError> {
        let directory = self.config.worlds_dir.join(provider_id.as_str());
        let connection = LibvirtConnection::open()?;
        let domain = match Domain::lookup_by_name(&connection, provider_id.as_str()) {
            Ok(domain) => Some(domain),
            Err(error) if error.code() == ErrorNumber::NoDomain => None,
            Err(error) => return Err(context("look up libvirt domain", error)),
        };
        match (domain, directory.exists()) {
            (None, false) => Ok(None),
            (None, true) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: files exist but domain is missing",
                provider_id
            ))),
            (Some(_), false) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: domain exists but files are missing",
                provider_id
            ))),
            (Some(domain), true) => {
                let paths = world::Paths::new(&self.config.worlds_dir, provider_id);
                if [
                    &paths.disk,
                    &paths.seed,
                    &paths.user_data,
                    &paths.meta_data,
                    &paths.network_config,
                ]
                .into_iter()
                .any(|path| !path.is_file())
                {
                    return Err(WorkerError::new(format!(
                        "partial libvirt machine {provider_id}: required machine files are missing"
                    )));
                }
                if !domain
                    .is_active()
                    .map_err(|error| context("check domain state", error))?
                {
                    return Err(WorkerError::new(format!(
                        "libvirt machine {provider_id} is stopped"
                    )));
                }
                domain
                    .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                    .map_err(|error| context("contact QEMU guest agent", error))?;
                let guest_ip = domain_ip(provider_id)?.ok_or_else(|| {
                    WorkerError::new(format!("libvirt machine {provider_id} has no IPv4 address"))
                })?;
                Ok(Some(self.machine(provider_id, guest_ip)))
            }
        }
    }

    fn delete(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        self.cleanup(provider_id)
    }
}

pub(super) fn lookup_domain(provider_id: &ProviderId) -> Result<Domain, WorkerError> {
    let connection = LibvirtConnection::open()?;
    Domain::lookup_by_name(&connection, provider_id.as_str())
        .map_err(|error| context("look up libvirt domain", error))
}

fn domain_ip(provider_id: &ProviderId) -> Result<Option<String>, WorkerError> {
    let domain = lookup_domain(provider_id)?;
    let interfaces = domain
        .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
        .map_err(|error| context("get domain interface addresses", error))?;
    Ok(interfaces
        .into_iter()
        .flat_map(|interface| interface.addrs)
        .find_map(|address| {
            let ip = address.addr.parse::<std::net::IpAddr>().ok()?;
            (ip.is_ipv4() && !ip.is_loopback()).then(|| ip.to_string())
        }))
}

fn network_address(connection: &Connect, name: &str) -> Result<String, WorkerError> {
    let network = Network::lookup_by_name(connection, name)
        .map_err(|error| context("look up libvirt network", error))?;
    let xml = network
        .get_xml_desc(0)
        .map_err(|error| context("read libvirt network XML", error))?;
    for quote in ['\'', '"'] {
        let needle = format!("address={quote}");
        for rest in xml.split(&needle).skip(1) {
            if let Some(address) = rest.split(quote).next() {
                if address.parse::<std::net::Ipv4Addr>().is_ok() {
                    return Ok(address.to_owned());
                }
            }
        }
    }
    Err(WorkerError::new(
        "configured libvirt network has no IPv4 bridge address",
    ))
}

fn require_file(path: &std::path::Path, label: &str) -> Result<(), WorkerError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "{label} not found: {}",
            path.display()
        )))
    }
}

fn prepare_qemu_file_access(paths: &world::Paths) -> Result<(), WorkerError> {
    for (path, mode, action) in [
        (
            &paths.directory,
            0o2770,
            "set machine directory permissions",
        ),
        (&paths.disk, 0o660, "set qcow2 overlay permissions"),
        (&paths.seed, 0o640, "set cloud-init seed permissions"),
    ] {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| context(action, error))?;
    }
    Ok(())
}

fn run(mut command: Command, action: &str) -> Result<(), WorkerError> {
    let output = command.output().map_err(|error| context(action, error))?;
    if output.status.success() {
        return Ok(());
    }
    Err(WorkerError::new(format!(
        "{action}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}
