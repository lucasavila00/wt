use crate::{GuestAccess, HostConfig, GUEST_GROUP, GUEST_USER};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_control_protocol::SshAccess;
use wt_libvirt_kvm::{
    GuestTransport, Machine, MachineInspection, MachineProvider, MachineSpec, ProviderId,
    RunRequest, WorkerError, WriteFileRequest,
};

const PREPARE: &str = "/usr/local/libexec/wt-guest-prepare";
const CODEX_RECONCILIATION_DESIRED: &str = "/home/wt/.local/state/wt/codex-reconciliation-desired";
const CODEX_RECONCILIATION_STAGED: &str =
    "/home/wt/.local/state/wt/.codex-reconciliation-desired.next";
const CODEX_RECONCILIATION_SERVICE: &str = "wt-codex-reconciliation.service";

pub struct ProvisionSpec<'a> {
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
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
    config: HostConfig,
}

impl<P> Worker<P> {
    pub fn new(
        provider: P,
        readiness_timeout: Duration,
        config: HostConfig,
    ) -> Result<Self, WorkerError> {
        config.validate()?;
        Ok(Self {
            provider,
            readiness_timeout,
            config,
        })
    }
}

impl<P: MachineProvider> Worker<P> {
    pub fn request_codex_reconciliation(
        &self,
        backend_id: &str,
        generation: &str,
    ) -> Result<bool, WorkerError> {
        if generation.len() != 64
            || !generation
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(WorkerError::new(
                "Codex catalog generation must be 64 lowercase hexadecimal characters",
            ));
        }
        let machine = match self.provider.inspect(&ProviderId::parse(backend_id)?)? {
            MachineInspection::Running(machine) => machine,
            MachineInspection::Missing | MachineInspection::Stopped { .. } => return Ok(false),
        };
        let deadline = Instant::now() + self.readiness_timeout;
        let desired = format!("{generation}\n");
        machine.transport.write_file(&WriteFileRequest {
            path: CODEX_RECONCILIATION_STAGED,
            contents: desired.as_bytes(),
            owner: GUEST_USER,
            group: GUEST_GROUP,
            mode: 0o600,
            deadline,
        })?;
        let output = machine.transport.run(
            &RunRequest {
                executable: "/bin/mv",
                args: &[
                    "--force",
                    "--",
                    CODEX_RECONCILIATION_STAGED,
                    CODEX_RECONCILIATION_DESIRED,
                ],
                stdin: None,
                deadline,
            },
            &mut std::io::sink(),
        )?;
        if output.exit_code != 0 {
            return Err(WorkerError::new(format!(
                "publish Codex reconciliation request: exit code {}: {}",
                output.exit_code,
                String::from_utf8_lossy(&output.diagnostic_tail).trim()
            )));
        }
        let output = machine.transport.run(
            &RunRequest {
                executable: "/usr/bin/systemctl",
                args: &["start", "--no-block", CODEX_RECONCILIATION_SERVICE],
                stdin: None,
                deadline,
            },
            &mut std::io::sink(),
        )?;
        if output.exit_code != 0 {
            return Err(WorkerError::new(format!(
                "start Codex reconciliation: exit code {}: {}",
                output.exit_code,
                String::from_utf8_lossy(&output.diagnostic_tail).trim()
            )));
        }
        Ok(true)
    }
}

impl<P: MachineProvider> crate::WorldWorker for Worker<P> {
    fn provision(
        &self,
        spec: ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<GuestAccess, WorkerError> {
        crate::verify_pinned_image_identity(self.provider.image_path())?;
        let creation_started = Instant::now();
        let phase_started = Instant::now();
        let readiness_key = ReadinessKey::generate()?;
        crate::write_creation_timing(log, "generate readiness key", phase_started.elapsed())?;
        let authorized_keys = [readiness_key.public_key.clone()];
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
        self.config.provision(
            machine.transport.as_ref(),
            crate::guest::ProvisionSpec {
                authorized_keys: &authorized_keys,
                git_user_name: spec.git_user_name,
                git_user_email: spec.git_user_email,
                git_grant: spec.git_grant,
            },
            deadline,
            log,
        )?;
        crate::write_creation_timing(log, "configure guest", phase_started.elapsed())?;
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
        self.config.mount_codex(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldWorker;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct FakeProvider {
        image: PathBuf,
        create_calls: Arc<AtomicUsize>,
    }

    impl MachineProvider for FakeProvider {
        fn image_path(&self) -> &Path {
            &self.image
        }

        fn create(
            &self,
            _spec: &MachineSpec,
            _progress: &mut dyn Write,
        ) -> Result<Machine, WorkerError> {
            self.create_calls.fetch_add(1, Ordering::SeqCst);
            Err(WorkerError::new("unexpected machine creation"))
        }

        fn inspect(&self, _provider_id: &ProviderId) -> Result<MachineInspection, WorkerError> {
            unreachable!()
        }

        fn start(&self, _provider_id: &ProviderId) -> Result<Machine, WorkerError> {
            unreachable!()
        }

        fn stop(&self, _provider_id: &ProviderId) -> Result<(), WorkerError> {
            unreachable!()
        }

        fn disk_usage(&self, _disk_id: Uuid) -> Result<u64, WorkerError> {
            unreachable!()
        }

        fn delete(&self, _provider_id: &ProviderId, _disk_id: Uuid) -> Result<(), WorkerError> {
            unreachable!()
        }
    }

    #[test]
    fn identity_mismatch_fails_before_machine_creation() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("host.qcow2");
        fs::write(
            crate::guest::image_manifest_path(&image),
            r#"{"guest_identity":{"uid":1000,"gid":1000}}"#,
        )
        .unwrap();
        let create_calls = Arc::new(AtomicUsize::new(0));
        let worker = Worker::new(
            FakeProvider {
                image,
                create_calls: Arc::clone(&create_calls),
            },
            Duration::from_secs(1),
            HostConfig {
                agent_tools: crate::AgentToolsConfig {
                    provider_hosts: vec!["github.com".to_owned()],
                    vsock_port: 18017,
                },
            },
        )
        .unwrap();
        let error = worker
            .provision(
                ProvisionSpec {
                    backend_id: "wt-0123456789abcdef0123456789abcdef",
                    disk_id: Uuid::nil(),
                    memory_mib: 1024,
                    vcpus: 1,
                    disk_gib: 16,
                    git_grant: "grant",
                    git_user_name: "WT",
                    git_user_email: "wt@example.com",
                },
                &mut std::io::sink(),
            )
            .unwrap_err();

        assert_eq!(create_calls.load(Ordering::SeqCst), 0);
        insta::assert_snapshot!(error.to_string(), @"guest image guest identity mismatch: expected UID/GID 1001:1001, got 1000:1000");
    }
}
