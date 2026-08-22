use crate::{GuestAccess, RetainedConfig};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_control_protocol::SshAccess;
use wt_libvirt_kvm::{
    GuestTransport, Machine, MachineInspection, MachineProvider, MachineSpec, ProviderId,
    RunRequest, WorkerError,
};

const PREPARE: &str = "/usr/local/libexec/wt-host-prepare";

pub struct ProvisionSpec<'a> {
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub ssh_authorized_keys: &'a [String],
    pub git_grant: &'a str,
    pub git_user_name: &'a str,
    pub git_user_email: &'a str,
}

#[derive(Clone, Debug)]
pub enum WorldInspection {
    Missing,
    Running(GuestAccess),
    Stopped { reason: Option<String> },
}

#[derive(Clone)]
pub struct Worker<P> {
    provider: P,
    readiness_timeout: Duration,
    retained: RetainedConfig,
}

impl<P> Worker<P> {
    pub fn new(
        provider: P,
        readiness_timeout: Duration,
        retained: RetainedConfig,
    ) -> Result<Self, WorkerError> {
        retained.validate()?;
        Ok(Self {
            provider,
            readiness_timeout,
            retained,
        })
    }
}

impl<P: MachineProvider> crate::WorldWorker for Worker<P> {
    fn provision(
        &self,
        spec: ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<GuestAccess, WorkerError> {
        let creation_started = Instant::now();
        let phase_started = Instant::now();
        let readiness_key = ReadinessKey::generate()?;
        crate::write_creation_timing(log, "generate readiness key", phase_started.elapsed())?;
        let mut authorized_keys = spec.ssh_authorized_keys.to_vec();
        authorized_keys.push(readiness_key.public_key.clone());
        let phase_started = Instant::now();
        let machine = self.provider.create(
            &MachineSpec {
                provider_id: ProviderId::parse(spec.backend_id)?,
                disk_id: spec.disk_id,
                memory_mib: spec.memory_mib,
                vcpus: spec.vcpus,
                disk_gib: spec.disk_gib,
            },
            log,
        )?;
        crate::write_creation_timing(log, "create and boot machine", phase_started.elapsed())?;
        let deadline = Instant::now() + self.readiness_timeout;
        let phase_started = Instant::now();
        run_prepare(machine.transport.as_ref(), "wait", None, deadline, log)?;
        crate::write_creation_timing(log, "wait for guest system", phase_started.elapsed())?;
        let phase_started = Instant::now();
        run_prepare(
            machine.transport.as_ref(),
            "access-policy",
            None,
            deadline,
            log,
        )?;
        crate::write_creation_timing(log, "apply guest access policy", phase_started.elapsed())?;
        let phase_started = Instant::now();
        self.retained.provision(
            machine.transport.as_ref(),
            crate::retained::ProvisionSpec {
                authorized_keys: &authorized_keys,
                git_user_name: spec.git_user_name,
                git_user_email: spec.git_user_email,
                git_grant: spec.git_grant,
            },
            deadline,
            log,
        )?;
        crate::write_creation_timing(log, "configure retained guest", phase_started.elapsed())?;
        let phase_started = Instant::now();
        let access = inspect_machine(&machine, self.readiness_timeout, log)?;
        crate::write_creation_timing(log, "inspect guest SSH", phase_started.elapsed())?;
        let phase_started = Instant::now();
        verify_login(
            access.ssh(),
            readiness_key.private_key(),
            readiness_key.path(),
        )?;
        crate::write_creation_timing(log, "verify guest SSH login", phase_started.elapsed())?;
        let phase_started = Instant::now();
        run_prepare(
            machine.transport.as_ref(),
            "remove-key",
            Some(readiness_key.public_key.as_bytes()),
            Instant::now() + self.readiness_timeout,
            log,
        )?;
        crate::write_creation_timing(log, "remove readiness key", phase_started.elapsed())?;
        crate::write_creation_timing(log, "total server provisioning", creation_started.elapsed())?;
        Ok(access)
    }

    fn destroy(&self, backend_id: &str, disk_id: Uuid) -> Result<(), WorkerError> {
        self.provider
            .delete(&ProviderId::parse(backend_id)?, disk_id)
    }

    fn inspect(&self, backend_id: &str) -> Result<WorldInspection, WorkerError> {
        match self.provider.inspect(&ProviderId::parse(backend_id)?)? {
            MachineInspection::Missing => Ok(WorldInspection::Missing),
            MachineInspection::Stopped { reason } => Ok(WorldInspection::Stopped { reason }),
            MachineInspection::Running(machine) => {
                inspect_machine(&machine, self.readiness_timeout, &mut std::io::sink())
                    .map(WorldInspection::Running)
            }
        }
    }

    fn start(&self, backend_id: &str) -> Result<GuestAccess, WorkerError> {
        let machine = self.provider.start(&ProviderId::parse(backend_id)?)?;
        run_prepare(
            machine.transport.as_ref(),
            "wait",
            None,
            Instant::now() + self.readiness_timeout,
            &mut std::io::sink(),
        )?;
        self.retained.mount_codex(
            machine.transport.as_ref(),
            Instant::now() + self.readiness_timeout,
            &mut std::io::sink(),
        )?;
        inspect_machine(&machine, self.readiness_timeout, &mut std::io::sink())
    }

    fn stop(&self, backend_id: &str) -> Result<(), WorkerError> {
        self.provider.stop(&ProviderId::parse(backend_id)?)
    }

    fn disk_usage(&self, disk_id: Uuid) -> Result<u64, WorkerError> {
        self.provider.disk_usage(disk_id)
    }
}

struct ReadinessKey {
    directory: tempfile::TempDir,
    public_key: String,
}

impl ReadinessKey {
    fn generate() -> Result<Self, WorkerError> {
        let directory = tempfile::tempdir().map_err(|error| {
            WorkerError::new(format!("create SSH readiness directory: {error}"))
        })?;
        let private_key = directory.path().join("key");
        let output = Command::new("/usr/bin/ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&private_key)
            .output()
            .map_err(|error| WorkerError::new(format!("generate SSH readiness key: {error}")))?;
        if !output.status.success() {
            return Err(WorkerError::new(format!(
                "generate SSH readiness key: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let public_key = fs::read_to_string(private_key.with_extension("pub"))
            .map_err(|error| WorkerError::new(format!("read SSH readiness key: {error}")))?
            .trim()
            .to_owned();
        let mut public_key = ssh_key::PublicKey::from_openssh(&public_key)
            .map_err(|error| WorkerError::new(format!("parse SSH readiness key: {error}")))?;
        public_key.set_comment("");
        let public_key = public_key
            .to_openssh()
            .map_err(|error| WorkerError::new(format!("encode SSH readiness key: {error}")))?;
        Ok(Self {
            directory,
            public_key,
        })
    }

    fn private_key(&self) -> PathBuf {
        self.directory.path().join("key")
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }
}

fn inspect_machine(
    machine: &Machine,
    timeout: Duration,
    _log: &mut dyn Write,
) -> Result<GuestAccess, WorkerError> {
    let deadline = Instant::now() + timeout;
    let host_keys = crate::read_host_keys(machine.transport.as_ref(), deadline)?;
    crate::verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
    Ok(GuestAccess::from_guest_ip(
        machine.guest_ip.clone(),
        host_keys,
    ))
}

fn run_prepare(
    transport: &dyn GuestTransport,
    action: &str,
    stdin: Option<&[u8]>,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    let output = transport.run(
        &RunRequest {
            executable: PREPARE,
            args: &[action],
            stdin,
            deadline,
        },
        log,
    )?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "prepare host {action}: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.diagnostic_tail).trim()
        )));
    }
    Ok(())
}

fn verify_login(
    ssh: &SshAccess,
    private_key: PathBuf,
    directory: &Path,
) -> Result<(), WorkerError> {
    let known_hosts = directory.join("known_hosts");
    let contents = crate::normalized_host_keys(&ssh.host_keys.join("\n"))
        .into_iter()
        .map(|key| format!("{} {key}\n", ssh.host))
        .collect::<String>();
    if contents.is_empty() {
        return Err(WorkerError::new("SSH login readiness: no valid host keys"));
    }
    fs::write(&known_hosts, contents)
        .map_err(|error| WorkerError::new(format!("write SSH readiness known hosts: {error}")))?;
    let output = Command::new("/usr/bin/ssh")
        .args([
            "-F",
            "/dev/null",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "StrictHostKeyChecking=yes",
        ])
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
        .args(["-i"])
        .arg(private_key)
        .args([
            "-p",
            &ssh.port.to_string(),
            &format!("{}@{}", ssh.user, ssh.host),
            "true",
        ])
        .output()
        .map_err(|error| WorkerError::new(format!("verify SSH login readiness: {error}")))?;
    if !output.status.success() {
        return Err(WorkerError::new(format!(
            "SSH login readiness failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
