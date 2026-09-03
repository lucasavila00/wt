use crate::{GuestAccess, HostConfig};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use wt_control_protocol::SshAccess;
use wt_libvirt_kvm::{
    CaptureRequest, GuestTransport, Machine, MachineInspection, MachineProvider, MachineSpec,
    RunRequest, WorkerError,
};
use wt_world::WorldId;

const PREPARE: &str = "/usr/local/libexec/wt-guest-prepare";
const CODEX_CAPTURE_BYTES: usize = 128 * 1024 * 1024;
const CODEX_TURN_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

pub struct CodexTurnRequest<'a> {
    pub message: &'a str,
    pub session_id: Option<uuid::Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexTurnOutput {
    pub session_id: Option<uuid::Uuid>,
    pub result: Result<String, String>,
}

pub struct WorldProvisionSpec<'a> {
    pub world_id: WorldId,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
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
    pub fn run_codex_turn(
        &self,
        world_id: WorldId,
        request: CodexTurnRequest<'_>,
    ) -> Result<CodexTurnOutput, WorkerError> {
        let machine = match self.provider.inspect(world_id)? {
            MachineInspection::Running(machine) => machine,
            MachineInspection::Stopped { .. } => {
                return Err(WorkerError::new("world is stopped"));
            }
            MachineInspection::Missing => return Err(WorkerError::new("world is missing")),
        };
        let mut codex_args = vec!["exec"];
        if request.session_id.is_some() {
            codex_args.push("resume");
        }
        codex_args.extend([
            "--json",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
        ]);
        let session_id = request.session_id.map(|id| id.to_string());
        if let Some(thread_id) = session_id.as_deref() {
            codex_args.push(thread_id);
        }
        codex_args.push("-");
        let mut args = vec![
            "-H",
            "-u",
            crate::GUEST_USER,
            "--",
            "/bin/sh",
            "-c",
            "cd /home/wt && exec /usr/local/bin/codex \"$@\"",
            "wt-codex",
        ];
        args.extend(codex_args);
        let output = machine
            .transport
            .capture(&CaptureRequest {
                executable: "/usr/bin/sudo",
                args: &args,
                stdin: Some(request.message.as_bytes()),
                deadline: Instant::now() + CODEX_TURN_TIMEOUT,
                stdout_limit: CODEX_CAPTURE_BYTES,
                stderr_limit: CODEX_CAPTURE_BYTES,
            })
            .map_err(|error| WorkerError::new(format!("run Codex: {error}")))?;
        parse_codex_output(
            output.exit_code,
            &output.stdout,
            &output.stderr,
            request.session_id,
        )
    }
}

fn parse_codex_output(
    exit_code: i64,
    stdout: &[u8],
    stderr: &[u8],
    requested_session_id: Option<uuid::Uuid>,
) -> Result<CodexTurnOutput, WorkerError> {
    let mut thread_id = None;
    let mut message = None;
    for line in stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let event: serde_json::Value = serde_json::from_slice(line)
            .map_err(|error| WorkerError::new(format!("decode Codex JSONL: {error}")))?;
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("thread.started") => {
                thread_id = event
                    .get("thread_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::parse)
                    .transpose()
                    .map_err(|error| {
                        WorkerError::new(format!("invalid Codex thread ID: {error}"))
                    })?;
            }
            Some("item.completed")
                if event
                    .pointer("/item/type")
                    .and_then(serde_json::Value::as_str)
                    == Some("agent_message") =>
            {
                message = event
                    .pointer("/item/text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
            }
            _ => {}
        }
    }
    let session_id = thread_id.or(requested_session_id);
    let result = if exit_code != 0 {
        let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            format!("Codex exited with status {exit_code}")
        } else {
            format!("Codex exited with status {exit_code}: {stderr}")
        })
    } else {
        message.ok_or_else(|| "Codex did not return a final message".to_owned())
    };
    Ok(CodexTurnOutput { session_id, result })
}

impl<P: MachineProvider> crate::WorldWorker for Worker<P> {
    fn run_codex_turn(
        &self,
        world_id: WorldId,
        request: CodexTurnRequest<'_>,
    ) -> Result<CodexTurnOutput, WorkerError> {
        Self::run_codex_turn(self, world_id, request)
    }

    fn provision(
        &self,
        spec: WorldProvisionSpec<'_>,
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
                world_id: spec.world_id,
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

    fn destroy(&self, world_id: WorldId) -> Result<(), WorkerError> {
        self.provider.delete(world_id)
    }

    fn inspect(&self, world_id: WorldId) -> Result<WorldInspection, WorkerError> {
        match self.provider.inspect(world_id)? {
            MachineInspection::Missing => Ok(WorldInspection::Missing),
            MachineInspection::Stopped { reason } => Ok(WorldInspection::Stopped { reason }),
            MachineInspection::Running(machine) => {
                inspect_machine(&machine, self.readiness_timeout, &mut std::io::sink())
                    .map(WorldInspection::Running)
            }
        }
    }

    fn start(&self, world_id: WorldId) -> Result<GuestAccess, WorkerError> {
        let machine = self.provider.start(world_id)?;
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

    fn stop(&self, world_id: WorldId) -> Result<(), WorkerError> {
        self.provider.stop(world_id)
    }

    fn disk_usage(&self, world_id: WorldId) -> Result<u64, WorkerError> {
        self.provider.disk_usage(world_id)
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

        fn inspect(&self, _world_id: WorldId) -> Result<MachineInspection, WorkerError> {
            unreachable!()
        }

        fn start(&self, _world_id: WorldId) -> Result<Machine, WorkerError> {
            unreachable!()
        }

        fn stop(&self, _world_id: WorldId) -> Result<(), WorkerError> {
            unreachable!()
        }

        fn disk_usage(&self, _world_id: WorldId) -> Result<u64, WorkerError> {
            unreachable!()
        }

        fn delete(&self, _world_id: WorldId) -> Result<(), WorkerError> {
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
                WorldProvisionSpec {
                    world_id: WorldId::from(uuid::Uuid::nil()),
                    memory_mib: 1024,
                    vcpus: 1,
                    disk_gib: 16,
                    git_user_name: "WT",
                    git_user_email: "wt@example.com",
                },
                &mut std::io::sink(),
            )
            .unwrap_err();

        assert_eq!(create_calls.load(Ordering::SeqCst), 0);
        insta::assert_snapshot!(error.to_string(), @"guest image guest identity mismatch: expected UID/GID 1001:1001, got 1000:1000");
    }

    #[test]
    fn codex_jsonl_yields_the_thread_and_final_message() {
        let session_id = uuid::Uuid::new_v4();
        let output = parse_codex_output(
            0,
            format!(
                "{{\"type\":\"thread.started\",\"thread_id\":\"{session_id}\"}}\n\
                 {{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"done\"}}}}\n\
                 {{\"type\":\"turn.completed\"}}\n"
            )
            .as_bytes(),
            b"",
            None,
        )
        .unwrap();

        assert_eq!(
            output,
            CodexTurnOutput {
                session_id: Some(session_id),
                result: Ok("done".into()),
            }
        );
    }

    #[test]
    fn codex_failure_is_not_mistaken_for_a_terminal_message() {
        let session_id = uuid::Uuid::new_v4();
        let output =
            parse_codex_output(1, b"", b"authentication failed", Some(session_id)).unwrap();

        assert_eq!(
            output,
            CodexTurnOutput {
                session_id: Some(session_id),
                result: Err("Codex exited with status 1: authentication failed".into()),
            }
        );
    }
}
