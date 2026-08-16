use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_api::SshAccess;
use wt_provider::{
    CaptureRequest, GuestTransport, Machine, MachineInspection, MachineProvider, MachineSpec,
    NoCloudConfig, ProviderId, RunRequest, WorkerError, WriteFileRequest,
};

const CAPTURE_LIMIT: usize = 1024 * 1024;
const PREPARE: &str = "/usr/local/libexec/wt-host-prepare";
const INSPECT: &str = "/usr/local/libexec/wt-host-inspect";

pub struct ProvisionSpec<'a> {
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub ssh_authorized_keys: &'a [String],
    pub user_data: &'a str,
    pub git_grant: &'a str,
}

#[derive(Clone, Debug)]
pub struct AgentGitConfig {
    pub relay_binary: PathBuf,
    pub remote_binary: PathBuf,
    pub cli_binary: PathBuf,
    pub provider_hosts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
    pub ssh: SshAccess,
    pub setup_complete: bool,
}

#[derive(Clone, Debug)]
pub enum WorldInspection {
    Missing,
    Running(World),
    Stopped { reason: Option<String> },
}

pub trait WorldWorker {
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError>;
    fn destroy(&self, backend_id: &str, disk_ids: &[Uuid]) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<WorldInspection, WorkerError>;
    fn start(&self, backend_id: &str) -> Result<World, WorkerError>;
}

pub fn validate_user_data(user_data: &str) -> Result<(), String> {
    let mut document: serde_yaml_ng::Value = serde_yaml_ng::from_str(user_data)
        .map_err(|error| format!("cloud-init user-data is invalid YAML: {error}"))?;
    if document.is_null() {
        return Ok(());
    }
    document
        .apply_merge()
        .map_err(|error| format!("cloud-init user-data has an invalid YAML merge: {error}"))?;
    let Some(mapping) = document.as_mapping() else {
        return Err("cloud-init user-data must be a YAML mapping".to_owned());
    };
    for field in [
        "cloud_config_modules",
        "cloud_final_modules",
        "cloud_init_modules",
        "merge_how",
        "merge_type",
        "output",
        "ssh_deletekeys",
        "ssh_keys",
    ] {
        if mapping.contains_key(serde_yaml_ng::Value::String(field.to_owned())) {
            return Err(format!(
                "cloud-init user-data cannot set top-level {field}; WT owns host identity, setup stages, and output"
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct CompositeWorker<P> {
    provider: P,
    readiness_timeout: Duration,
    agent_git_relay: Vec<u8>,
    agent_git_remote: Vec<u8>,
    agent_git_cli: Vec<u8>,
    provider_hosts: Vec<u8>,
}

impl<P> CompositeWorker<P> {
    pub fn new(
        provider: P,
        readiness_timeout: Duration,
        agent_git: AgentGitConfig,
    ) -> Result<Self, WorkerError> {
        let read = |path: &Path, name: &str| {
            fs::read(path).map_err(|error| {
                WorkerError::new(format!("read host {name} {}: {error}", path.display()))
            })
        };
        Ok(Self {
            provider,
            readiness_timeout,
            agent_git_relay: read(&agent_git.relay_binary, "agent Git relay")?,
            agent_git_remote: read(&agent_git.remote_binary, "agent Git remote helper")?,
            agent_git_cli: read(&agent_git.cli_binary, "agent Git CLI")?,
            provider_hosts: format!("{}\n", agent_git.provider_hosts.join("\n")).into_bytes(),
        })
    }
}

impl<P: MachineProvider> WorldWorker for CompositeWorker<P> {
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        let readiness_key = ReadinessKey::generate()?;
        let mut authorized_keys = spec.ssh_authorized_keys.to_vec();
        authorized_keys.push(readiness_key.public_key.clone());
        let authorized_keys = authorized_keys_file(&authorized_keys)?;
        let machine = self.provider.create(
            &MachineSpec {
                provider_id: ProviderId::parse(spec.backend_id)?,
                disk_id: spec.disk_id,
                memory_mib: spec.memory_mib,
                vcpus: spec.vcpus,
                disk_gib: spec.disk_gib,
                cloud_init: NoCloudConfig::default(),
            },
            log,
        )?;
        let deadline = Instant::now() + self.readiness_timeout;
        run_prepare(machine.transport.as_ref(), "wait", None, deadline, log)?;
        run_prepare(
            machine.transport.as_ref(),
            "access",
            Some(authorized_keys.as_bytes()),
            deadline,
            log,
        )?;
        install_agent_git(
            machine.transport.as_ref(),
            spec.git_grant,
            &self.agent_git_relay,
            &self.agent_git_remote,
            &self.agent_git_cli,
            &self.provider_hosts,
            deadline,
            log,
        )?;
        run_prepare(
            machine.transport.as_ref(),
            "user-data",
            Some(spec.user_data.as_bytes()),
            deadline,
            log,
        )?;
        let world = inspect_machine(&machine, self.readiness_timeout, log)?;
        verify_login(
            &world.ssh,
            readiness_key.private_key(),
            readiness_key.path(),
        )?;
        run_prepare(
            machine.transport.as_ref(),
            "remove-key",
            Some(readiness_key.public_key.as_bytes()),
            Instant::now() + self.readiness_timeout,
            log,
        )?;
        Ok(world)
    }

    fn destroy(&self, backend_id: &str, disk_ids: &[Uuid]) -> Result<(), WorkerError> {
        self.provider
            .delete(&ProviderId::parse(backend_id)?, disk_ids)
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

    fn start(&self, backend_id: &str) -> Result<World, WorkerError> {
        let machine = self.provider.start(&ProviderId::parse(backend_id)?)?;
        inspect_machine(&machine, self.readiness_timeout, &mut std::io::sink())
    }
}

#[allow(clippy::too_many_arguments)]
fn install_agent_git(
    transport: &dyn GuestTransport,
    grant: &str,
    relay: &[u8],
    remote: &[u8],
    cli: &[u8],
    provider_hosts: &[u8],
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    for (path, contents) in [
        ("/tmp/wt-host-agent-git-grant", grant.as_bytes()),
        ("/tmp/wt-host-agent-git-relay", relay),
        ("/tmp/wt-host-agent-git-remote", remote),
        ("/tmp/wt-host-ag-git", cli),
        ("/tmp/wt-host-agent-git-providers", provider_hosts),
    ] {
        transport
            .write_file(&WriteFileRequest {
                path,
                contents,
                owner: "root",
                group: "root",
                mode: 0o600,
                deadline,
            })
            .map_err(WorkerError::from)?;
    }
    run_prepare(transport, "agent-git", None, deadline, log)
}

pub fn authorized_keys_file(authorized_keys: &[String]) -> Result<String, WorkerError> {
    let keys = authorized_keys
        .iter()
        .map(|key| {
            let mut key = ssh_key::PublicKey::from_openssh(key)
                .map_err(|error| WorkerError::new(format!("parse SSH authorized key: {error}")))?;
            key.set_comment("");
            let key = key
                .to_openssh()
                .map_err(|error| WorkerError::new(format!("encode SSH authorized key: {error}")))?;
            Ok(key)
        })
        .collect::<Result<Vec<_>, WorkerError>>()?
        .join("\n");
    Ok(format!("{keys}\n"))
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
) -> Result<World, WorkerError> {
    let deadline = Instant::now() + timeout;
    let setup = machine.transport.capture(&CaptureRequest {
        executable: INSPECT,
        args: &[],
        stdin: None,
        deadline,
        stdout_limit: CAPTURE_LIMIT,
        stderr_limit: CAPTURE_LIMIT,
    })?;
    if setup.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "inspect host setup: exit code {}: {}",
            setup.exit_code,
            String::from_utf8_lossy(&setup.stderr).trim()
        )));
    }
    let setup = String::from_utf8(setup.stdout).map_err(|error| {
        WorkerError::new(format!("inspect host setup returned non-UTF-8: {error}"))
    })?;
    let (state, detail) = setup.split_once('\n').unwrap_or((&setup, ""));
    let setup_complete = match state.trim() {
        "setup" => false,
        "complete" => true,
        "error" => {
            return Err(WorkerError::new(format!(
                "host cloud-init failed: {}",
                detail.trim()
            )))
        }
        other => {
            return Err(WorkerError::new(format!(
                "inspect host setup returned unknown state {other:?}"
            )))
        }
    };
    let host_keys = read_host_keys(machine.transport.as_ref(), deadline)?;
    verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
    Ok(World {
        guest_ip: machine.guest_ip.clone(),
        ssh: SshAccess {
            user: "wt".to_owned(),
            host: machine.guest_ip.clone(),
            port: 22,
            host_keys,
        },
        setup_complete,
    })
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

fn read_host_keys(
    transport: &dyn GuestTransport,
    deadline: Instant,
) -> Result<Vec<String>, WorkerError> {
    let output = transport.capture(&CaptureRequest {
        executable: "/bin/sh",
        args: &["-c", "cat /etc/ssh/ssh_host_*_key.pub"],
        stdin: None,
        deadline,
        stdout_limit: CAPTURE_LIMIT,
        stderr_limit: CAPTURE_LIMIT,
    })?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "SSH host keys: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let keys = String::from_utf8_lossy(&output.stdout)
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

fn verify_guest_ssh(
    guest_ip: &str,
    expected: &[String],
    deadline: Instant,
) -> Result<(), WorkerError> {
    let address: SocketAddr = format!("{guest_ip}:22")
        .parse()
        .map_err(|error| WorkerError::new(format!("parse guest SSH address: {error}")))?;
    while TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_err() {
        if Instant::now() >= deadline {
            return Err(WorkerError::new(
                "SSH readiness: timed out waiting for port 22",
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let output = Command::new("/usr/bin/ssh-keyscan")
        .args(["-T", "5", "-p", "22", guest_ip])
        .output()
        .map_err(|error| WorkerError::new(format!("scan guest SSH host keys: {error}")))?;
    let expected = normalized_keys(&expected.join("\n"));
    let presented = normalized_keys(&String::from_utf8_lossy(&output.stdout));
    if expected.is_disjoint(&presented) {
        return Err(WorkerError::new(format!(
            "SSH endpoint identity mismatch at {guest_ip}:22"
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
    let contents = normalized_keys(&ssh.host_keys.join("\n"))
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

fn normalized_keys(lines: &str) -> BTreeSet<String> {
    lines
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let first = fields.next()?;
            let (kind, data) = if first.starts_with("ssh-") || first.starts_with("ecdsa-") {
                (first, fields.next()?)
            } else {
                (fields.next()?, fields.next()?)
            };
            Some(format!("{kind} {data}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorized_keys_are_canonical() {
        insta::assert_snapshot!(authorized_keys_file(&[
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo person's-key".to_owned(),
        ]).unwrap(), @r###"
        ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo
        "###);
    }

    #[test]
    fn user_data_cannot_override_host_setup() {
        assert!(validate_user_data(
            "#cloud-config\nwrite_files:\n  - content: 'ssh_keys: allowed as text'\n"
        )
        .is_ok());
        for field in [
            "cloud_config_modules",
            "cloud_final_modules",
            "cloud_init_modules",
            "merge_how",
            "merge_type",
            "output",
            "ssh_deletekeys",
            "ssh_keys",
        ] {
            let error =
                validate_user_data(&format!("#cloud-config\n{field}: value\n")).unwrap_err();
            assert_eq!(
                error,
                format!(
                    "cloud-init user-data cannot set top-level {field}; WT owns host identity, setup stages, and output"
                )
            );
        }
        insta::assert_snapshot!(
            validate_user_data("#cloud-config\nsettings: &settings\n  ssh_keys: value\n<<: *settings\n")
                .unwrap_err(),
            @"cloud-init user-data cannot set top-level ssh_keys; WT owns host identity, setup stages, and output"
        );
        insta::assert_snapshot!(
            validate_user_data("#cloud-config\ninvalid: [\n").unwrap_err(),
            @"cloud-init user-data is invalid YAML: did not find expected node content at line 3 column 1, while parsing a flow node"
        );
    }
}
