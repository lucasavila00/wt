//! Libvirt/KVM machine lifecycle.

mod guest_agent;
mod image;
mod network;
mod provider;
mod world;

use crate::{MachineConfig, LIBVIRT_URI};
use image::{read_virtual_size as read_image_virtual_size, validate_disk_size};
use network::{domain_ip, network_address};
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;
use virt::network::Network;
use wt_command::cmd;
use wt_libvirt_kvm::{
    Machine, MachineInspection, MachineProvider, MachineSpec, ProviderId, WorkerError,
};

const GUEST_AGENT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const GUEST_IP_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(250);

struct LibvirtConnection(Connect);

impl LibvirtConnection {
    fn open() -> Result<Self, WorkerError> {
        // Libvirt's default callback prints every failed poll even though callers
        // handle and report the returned error with more useful context.
        virt::error::clear_error_callback();
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
    image_virtual_size: u64,
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
        let disks_dir = config.worlds_dir.join("disks");
        fs::create_dir_all(&disks_dir)
            .map_err(|error| context("create disk node directory", error))?;
        fs::set_permissions(&disks_dir, fs::Permissions::from_mode(0o2770))
            .map_err(|error| context("set disk node directory permissions", error))?;
        let connection = LibvirtConnection::open()?;
        Network::lookup_by_name(&connection, &config.network)
            .map_err(|error| context("look up libvirt network", error))?;
        let image_virtual_size = read_image_virtual_size(&config.image)?;
        Ok(Self {
            config,
            image_virtual_size,
        })
    }

    pub fn network_bridge_address(&self) -> Result<String, WorkerError> {
        let connection = LibvirtConnection::open()?;
        network_address(&connection, &self.config.network)
    }

    fn wait_for_agent(&self, provider_id: &ProviderId) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            let domain = lookup_domain(provider_id)?;
            let error = match domain.qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0) {
                Ok(_) => return Ok(()),
                Err(error) => error,
            };
            if Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for QEMU guest agent in domain {provider_id}; last libvirt error: {error}"
                )));
            }
            std::thread::sleep(GUEST_AGENT_POLL_INTERVAL);
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
            std::thread::sleep(GUEST_IP_POLL_INTERVAL);
        }
    }

    fn machine(&self, provider_id: &ProviderId, guest_ip: String) -> Machine {
        Machine {
            provider_id: provider_id.clone(),
            guest_ip,
            transport: Arc::new(guest_agent::QemuGuestTransport::new(provider_id.clone())),
        }
    }

    fn start_domain(
        &self,
        spec: &MachineSpec,
        disk: &std::path::Path,
        network_enabled: bool,
    ) -> Result<(), WorkerError> {
        let paths = world::Paths::new(&self.config.worlds_dir, &spec.provider_id);
        fs::create_dir(&paths.directory)
            .map_err(|error| context("create machine directory", error))?;
        fs::write(&paths.user_data, &spec.cloud_init.user_data)
            .map_err(|error| context("write cloud-init user-data", error))?;
        fs::write(&paths.vendor_data, &spec.cloud_init.vendor_data)
            .map_err(|error| context("write cloud-init vendor-data", error))?;
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
                "--vendor-data",
                &paths.vendor_data,
                &paths.seed,
                &paths.user_data,
                &paths.meta_data
            ),
            "create cloud-init seed",
        )?;
        prepare_qemu_file_access(&paths, disk)?;
        let xml = world::domain_xml(
            &spec.provider_id,
            &paths,
            disk,
            &self.config,
            spec,
            network_enabled,
        );
        let connection = LibvirtConnection::open()?;
        let domain = Domain::define_xml(&connection, &xml)
            .map_err(|error| context("define KVM domain", error))?;
        domain
            .create()
            .map_err(|error| context("start KVM domain", error))?;
        Ok(())
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

    fn remove_disk(&self, disk_id: uuid::Uuid) -> Result<(), WorkerError> {
        let path = world::disk_path(&self.config.worlds_dir, disk_id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(context(&format!("remove {}", path.display()), error)),
        }
    }

    fn allocated_disk_bytes(&self, disk_id: uuid::Uuid) -> Result<u64, WorkerError> {
        let path = world::disk_path(&self.config.worlds_dir, disk_id);
        allocated_bytes(&path)
    }

    fn cleanup(&self, provider_id: &ProviderId, disk_id: uuid::Uuid) -> Result<(), WorkerError> {
        let mut errors = Vec::new();
        if let Err(error) = self.remove_domain(provider_id) {
            errors.push(error.to_string());
        }
        if let Err(error) = self.remove_files(provider_id) {
            errors.push(error.to_string());
        }
        if let Err(error) = self.remove_disk(disk_id) {
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
        validate_disk_size(spec.disk_gib, self.image_virtual_size)?;
        writeln!(progress, "Creating KVM guest {}...", spec.provider_id)
            .map_err(|error| context("write machine progress", error))?;
        let disk = world::disk_path(&self.config.worlds_dir, spec.disk_id);
        run(
            copy_image_command(&self.config.image, &disk),
            "copy golden image into world disk",
        )?;
        run(
            resize_disk_command(&disk, spec.disk_gib),
            "resize world disk",
        )?;
        self.start_domain(spec, &disk, true)?;
        writeln!(progress, "Waiting for the guest transport...")
            .map_err(|error| context("write machine progress", error))?;
        self.wait_for_agent(&spec.provider_id)?;
        let guest_ip = self.wait_for_ip(&spec.provider_id)?;
        writeln!(progress, "Machine transport ready at {guest_ip}.")
            .map_err(|error| context("write machine progress", error))?;
        Ok(self.machine(&spec.provider_id, guest_ip))
    }
}

fn allocated_bytes(path: &std::path::Path) -> Result<u64, WorkerError> {
    let metadata = fs::metadata(path)
        .map_err(|error| context(&format!("inspect {}", path.display()), error))?;
    metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| WorkerError::new(format!("allocated size is too large: {}", path.display())))
}

fn copy_image_command(source: &std::path::Path, destination: &std::path::Path) -> Command {
    cmd!(
        "qemu-img",
        "convert",
        "-q",
        "-O",
        "qcow2",
        source,
        destination,
    )
}

fn resize_disk_command(disk: &std::path::Path, disk_gib: u64) -> Command {
    cmd!("qemu-img", "resize", "-q", disk, format!("{disk_gib}G"))
}

fn shutdown_reason(reason: i32) -> Option<&'static str> {
    match reason as u32 {
        virt::sys::VIR_DOMAIN_SHUTOFF_SHUTDOWN => Some("shutdown"),
        virt::sys::VIR_DOMAIN_SHUTOFF_DESTROYED => Some("destroyed"),
        virt::sys::VIR_DOMAIN_SHUTOFF_CRASHED => Some("crashed"),
        virt::sys::VIR_DOMAIN_SHUTOFF_MIGRATED => Some("migrated"),
        virt::sys::VIR_DOMAIN_SHUTOFF_SAVED => Some("saved"),
        virt::sys::VIR_DOMAIN_SHUTOFF_FAILED => Some("failed"),
        virt::sys::VIR_DOMAIN_SHUTOFF_FROM_SNAPSHOT => Some("from snapshot"),
        virt::sys::VIR_DOMAIN_SHUTOFF_DAEMON => Some("daemon"),
        _ => None,
    }
}

pub(super) fn lookup_domain(provider_id: &ProviderId) -> Result<Domain, WorkerError> {
    let connection = LibvirtConnection::open()?;
    Domain::lookup_by_name(&connection, provider_id.as_str())
        .map_err(|error| context("look up libvirt domain", error))
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

fn prepare_qemu_file_access(
    paths: &world::Paths,
    disk: &std::path::Path,
) -> Result<(), WorkerError> {
    for (path, mode, action) in [
        (
            paths.directory.as_path(),
            0o2770,
            "set machine directory permissions",
        ),
        (disk, 0o660, "set world disk permissions"),
        (
            paths.seed.as_path(),
            0o640,
            "set cloud-init seed permissions",
        ),
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

#[cfg(test)]
mod tests;
