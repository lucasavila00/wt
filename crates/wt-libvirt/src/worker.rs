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
use virt::domain_snapshot::DomainSnapshot;
use virt::error::ErrorNumber;
use virt::network::Network;
use wt_command::cmd;
use wt_provider::{
    CaptureRequest, ForkError, ForkMachineSpec, GuestTransport, Machine, MachineInspection,
    MachineProvider, MachineSpec, ProviderId, RunRequest, WorkerError,
};

const GUEST_AGENT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const GUEST_IP_POLL_INTERVAL: Duration = Duration::from_millis(250);

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
        let disks_dir = config.worlds_dir.join("disks");
        fs::create_dir_all(&disks_dir)
            .map_err(|error| context("create disk node directory", error))?;
        fs::set_permissions(&disks_dir, fs::Permissions::from_mode(0o2770))
            .map_err(|error| context("set disk node directory permissions", error))?;
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

    fn remove_disks(&self, disk_ids: &[uuid::Uuid]) -> Result<(), WorkerError> {
        let mut errors = Vec::new();
        for disk_id in disk_ids {
            let path = world::disk_path(&self.config.worlds_dir, *disk_id);
            if let Err(error) = fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("remove {}: {error}", path.display()));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(WorkerError::new(errors.join("; ")))
        }
    }

    fn cleanup(
        &self,
        provider_id: &ProviderId,
        disk_ids: &[uuid::Uuid],
    ) -> Result<(), WorkerError> {
        let mut errors = Vec::new();
        if let Err(error) = self.remove_domain(provider_id) {
            errors.push(error.to_string());
        }
        if let Err(error) = self.remove_files(provider_id) {
            errors.push(error.to_string());
        }
        if let Err(error) = self.remove_disks(disk_ids) {
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
        let disk = world::disk_path(&self.config.worlds_dir, spec.disk_id);
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
                &disk,
                format!("{}G", spec.disk_gib),
            ),
            "create qcow2 overlay",
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

    fn fork_inner(
        &self,
        spec: &ForkMachineSpec,
        progress: &mut dyn Write,
    ) -> Result<Machine, ForkError> {
        if spec.source_provider_id == spec.machine.provider_id
            || spec.source_disk_id == spec.source_head_disk_id
            || spec.source_disk_id == spec.machine.disk_id
            || spec.source_head_disk_id == spec.machine.disk_id
        {
            return Err(ForkError::before_pivot(WorkerError::new(
                "fork machine and disk identities must be distinct",
            )));
        }
        let source_disk = world::disk_path(&self.config.worlds_dir, spec.source_disk_id);
        let source_head = world::disk_path(&self.config.worlds_dir, spec.source_head_disk_id);
        let fork_head = world::disk_path(&self.config.worlds_dir, spec.machine.disk_id);
        require_file(&source_disk, "source disk node").map_err(ForkError::before_pivot)?;
        let source = lookup_domain(&spec.source_provider_id).map_err(ForkError::before_pivot)?;
        if !source
            .is_active()
            .map_err(|error| ForkError::before_pivot(context("check source domain state", error)))?
        {
            return Err(ForkError::before_pivot(WorkerError::new(
                "source libvirt machine is stopped",
            )));
        }
        let running_containers =
            running_containers(&spec.source_provider_id, self.config.boot_timeout)
                .map_err(ForkError::before_pivot)?;
        let source_xml = source
            .get_xml_desc(0)
            .map_err(|error| ForkError::before_pivot(context("read source domain XML", error)))?;
        let source_path = source_disk.to_string_lossy();
        if !source_xml.contains(&format!("file='{source_path}'"))
            && !source_xml.contains(&format!("file=\"{source_path}\""))
        {
            return Err(ForkError::before_pivot(WorkerError::new(
                "source domain does not use its registered disk head",
            )));
        }

        writeln!(
            progress,
            "Quiescing and pivoting source {}...",
            spec.source_provider_id
        )
        .map_err(|error| ForkError::before_pivot(context("write fork progress", error)))?;
        create_overlay(&source_disk, &source_head).map_err(ForkError::before_pivot)?;
        let source_head_text = source_head.to_string_lossy();
        let escaped = quick_xml::escape::escape(source_head_text.as_ref());
        let snapshot_xml = format!(
            "<domainsnapshot><memory snapshot='no'/><disks><disk name='vda' snapshot='external' type='file'><driver type='qcow2'/><source file='{escaped}'/></disk><disk name='sda' snapshot='no'/></disks></domainsnapshot>"
        );
        let flags = virt::sys::VIR_DOMAIN_SNAPSHOT_CREATE_DISK_ONLY
            | virt::sys::VIR_DOMAIN_SNAPSHOT_CREATE_NO_METADATA
            | virt::sys::VIR_DOMAIN_SNAPSHOT_CREATE_REUSE_EXT
            | virt::sys::VIR_DOMAIN_SNAPSHOT_CREATE_QUIESCE
            | virt::sys::VIR_DOMAIN_SNAPSHOT_CREATE_ATOMIC;
        let pivot = DomainSnapshot::create_xml(&source, &snapshot_xml, flags)
            .map(|_| ())
            .map_err(|error| context("quiesce and pivot source disk", error));
        let thaw = ensure_thawed(&source);
        if let Err(primary) = pivot {
            let _ = fs::remove_file(&source_head);
            return Err(ForkError::before_pivot(match thaw {
                Ok(()) => primary,
                Err(thaw) => {
                    WorkerError::new(format!("{primary}; source thaw also failed: {thaw}"))
                }
            }));
        }
        thaw.map_err(ForkError::after_pivot)?;
        let result = (|| {
            writeln!(
                progress,
                "Booting fork {} without network access...",
                spec.machine.provider_id
            )
            .map_err(|error| context("write fork progress", error))?;
            create_overlay(&source_disk, &fork_head)?;
            self.start_domain(&spec.machine, &fork_head, false)?;
            self.wait_for_agent(&spec.machine.provider_id)?;
            let machine = self.machine(&spec.machine.provider_id, String::new());
            replace_machine_identities(&machine, progress, self.config.boot_timeout)?;
            let domain = lookup_domain(&spec.machine.provider_id)?;
            let interface = world::interface_xml(&spec.machine.provider_id, &self.config);
            domain
                .attach_device_flags(
                    &interface,
                    virt::sys::VIR_DOMAIN_DEVICE_MODIFY_LIVE
                        | virt::sys::VIR_DOMAIN_DEVICE_MODIFY_CONFIG,
                )
                .map_err(|error| context("attach fork network", error))?;
            writeln!(progress, "Waiting for fork DHCP...")
                .map_err(|error| context("write fork progress", error))?;
            let guest_ip = self.wait_for_ip(&spec.machine.provider_id)?;
            restart_containers(
                &machine,
                &running_containers,
                progress,
                self.config.boot_timeout,
            )?;
            writeln!(progress, "Fork transport ready at {guest_ip}.")
                .map_err(|error| context("write fork progress", error))?;
            Ok(self.machine(&spec.machine.provider_id, guest_ip))
        })();
        result.map_err(ForkError::after_pivot)
    }
}

fn create_overlay(
    backing: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), WorkerError> {
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
            backing,
            destination,
        ),
        "create copy-on-write disk head",
    )?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o660))
        .map_err(|error| context("set copy-on-write disk permissions", error))
}

fn ensure_thawed(domain: &Domain) -> Result<(), WorkerError> {
    let mut last_error = None;
    for _ in 0..3 {
        match domain.qemu_agent_command(r#"{"execute":"guest-fsfreeze-status"}"#, 5, 0) {
            Ok(response) => match serde_json::from_str::<serde_json::Value>(&response) {
                Ok(response) if response["return"].as_str() == Some("thawed") => return Ok(()),
                Ok(_) => {}
                Err(error) => {
                    last_error = Some(context("decode source filesystem freeze status", error));
                }
            },
            Err(error) => {
                last_error = Some(context("read source filesystem freeze status", error));
            }
        }
        if let Err(error) = domain.qemu_agent_command(r#"{"execute":"guest-fsfreeze-thaw"}"#, 30, 0)
        {
            last_error = Some(context("thaw source filesystems", error));
        }
    }
    Err(last_error.unwrap_or_else(|| WorkerError::new("source filesystems remained frozen")))
}

fn replace_machine_identities(
    machine: &Machine,
    progress: &mut dyn Write,
    timeout: Duration,
) -> Result<(), WorkerError> {
    const SCRIPT: &str = r#"set -eu
name=$1
hostnamectl set-hostname "$name"
rm -f /var/lib/dbus/machine-id
truncate -s 0 /etc/machine-id
systemd-machine-id-setup
ln -sfn /etc/machine-id /var/lib/dbus/machine-id
rm -f /etc/ssh/ssh_host_*_key /etc/ssh/ssh_host_*_key.pub
ssh-keygen -A
if test -d /var/lib/wt-app-ssh; then
    old_session=$(cat /var/lib/wt-app-ssh/session_identity.pub)
    rm -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub
    rm -f /var/lib/wt-app-ssh/session_identity /var/lib/wt-app-ssh/session_identity.pub /var/lib/wt-app-ssh/known_hosts
    ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key
    ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/session_identity
    chown wt:wt /var/lib/wt-app-ssh/session_identity /var/lib/wt-app-ssh/session_identity.pub
    chmod 0600 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key /var/lib/wt-app-ssh/session_identity
    chmod 0644 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub /var/lib/wt-app-ssh/session_identity.pub
    new_session=$(cat /var/lib/wt-app-ssh/session_identity.pub)
    for keys in /var/lib/wt-app-ssh/public/authorized_keys/*; do
        test -f "$keys" || continue
        grep -Fvx "$old_session" "$keys" > "$keys.new" || true
        printf '%s\n' "$new_session" >> "$keys.new"
        chmod 0644 "$keys.new"
        mv "$keys.new" "$keys"
    done
fi
systemctl restart ssh.service
"#;
    writeln!(progress, "Replacing fork machine and SSH identities...")
        .map_err(|error| context("write fork progress", error))?;
    let result = machine.transport.run(
        &RunRequest {
            executable: "/bin/sh",
            args: &["-c", SCRIPT, "wt-fork", machine.provider_id.as_str()],
            stdin: None,
            deadline: Instant::now() + timeout,
        },
        progress,
    )?;
    if result.exit_code == 0 {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "replace fork identities: exit code {}: {}",
            result.exit_code,
            String::from_utf8_lossy(&result.diagnostic_tail).trim()
        )))
    }
}

fn running_containers(
    provider_id: &ProviderId,
    timeout: Duration,
) -> Result<Vec<String>, WorkerError> {
    let transport = guest_agent::QemuGuestTransport::new(provider_id.clone());
    let output = transport.capture(&CaptureRequest {
        executable: "/usr/bin/docker",
        args: &["ps", "--quiet", "--no-trunc"],
        stdin: None,
        deadline: Instant::now() + timeout,
        stdout_limit: 1024 * 1024,
        stderr_limit: 64 * 1024,
    })?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "list running source containers: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let id = std::str::from_utf8(line)
                .map_err(|error| context("decode running source container ID", error))?;
            if id.len() != 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(WorkerError::new("Docker returned an invalid container ID"));
            }
            Ok(id.to_owned())
        })
        .collect()
}

fn restart_containers(
    machine: &Machine,
    container_ids: &[String],
    progress: &mut dyn Write,
    timeout: Duration,
) -> Result<(), WorkerError> {
    if container_ids.is_empty() {
        return Err(WorkerError::new(
            "source world has no running Docker containers",
        ));
    }
    writeln!(progress, "Restarting fork containers...")
        .map_err(|error| context("write fork progress", error))?;
    let mut args = vec!["restart"];
    args.extend(container_ids.iter().map(String::as_str));
    let result = machine.transport.run(
        &RunRequest {
            executable: "/usr/bin/docker",
            args: &args,
            stdin: None,
            deadline: Instant::now() + timeout,
        },
        progress,
    )?;
    if result.exit_code == 0 {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "restart fork containers: exit code {}: {}",
            result.exit_code,
            String::from_utf8_lossy(&result.diagnostic_tail).trim()
        )))
    }
}

impl MachineProvider for LibvirtProvider {
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError> {
        match self.create_inner(spec, progress) {
            Ok(machine) => Ok(machine),
            Err(primary) => {
                if let Err(cleanup) = self.cleanup(&spec.provider_id, &[spec.disk_id]) {
                    Err(WorkerError::new(format!(
                        "{primary} (cleanup also failed: {cleanup})"
                    )))
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn fork(&self, spec: &ForkMachineSpec, progress: &mut dyn Write) -> Result<Machine, ForkError> {
        self.fork_inner(spec, progress)
    }

    fn inspect(&self, provider_id: &ProviderId) -> Result<MachineInspection, WorkerError> {
        let directory = self.config.worlds_dir.join(provider_id.as_str());
        let connection = LibvirtConnection::open()?;
        let domain = match Domain::lookup_by_name(&connection, provider_id.as_str()) {
            Ok(domain) => Some(domain),
            Err(error) if error.code() == ErrorNumber::NoDomain => None,
            Err(error) => return Err(context("look up libvirt domain", error)),
        };
        match (domain, directory.exists()) {
            (None, false) => Ok(MachineInspection::Missing),
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
                    let (_, reason) = domain
                        .get_state()
                        .map_err(|error| context("read stopped domain state", error))?;
                    return Ok(MachineInspection::Stopped {
                        reason: shutdown_reason(reason).map(str::to_owned),
                    });
                }
                domain
                    .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                    .map_err(|error| context("contact QEMU guest agent", error))?;
                let guest_ip = domain_ip(provider_id)?.ok_or_else(|| {
                    WorkerError::new(format!("libvirt machine {provider_id} has no IPv4 address"))
                })?;
                Ok(MachineInspection::Running(
                    self.machine(provider_id, guest_ip),
                ))
            }
        }
    }

    fn start(&self, provider_id: &ProviderId) -> Result<Machine, WorkerError> {
        match self.inspect(provider_id)? {
            MachineInspection::Missing => {
                return Err(WorkerError::new(format!(
                    "libvirt machine {provider_id} is missing"
                )))
            }
            MachineInspection::Running(machine) => return Ok(machine),
            MachineInspection::Stopped { .. } => {}
        }
        let domain = lookup_domain(provider_id)?;
        domain
            .create()
            .map_err(|error| context("start KVM domain", error))?;
        self.wait_for_agent(provider_id)?;
        let guest_ip = self.wait_for_ip(provider_id)?;
        Ok(self.machine(provider_id, guest_ip))
    }

    fn delete(&self, provider_id: &ProviderId, disk_ids: &[uuid::Uuid]) -> Result<(), WorkerError> {
        self.cleanup(provider_id, disk_ids)
    }
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
        (disk, 0o660, "set qcow2 overlay permissions"),
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
mod tests {
    use super::shutdown_reason;

    #[test]
    fn names_known_libvirt_shutdown_reasons() {
        assert_eq!(
            shutdown_reason(virt::sys::VIR_DOMAIN_SHUTOFF_CRASHED as i32),
            Some("crashed")
        );
        assert_eq!(shutdown_reason(-1), None);
    }
}
