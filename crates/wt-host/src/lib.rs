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
    NoCloudConfig, ProviderId, RunRequest, WorkerError,
};

const CAPTURE_LIMIT: usize = 1024 * 1024;

pub struct ProvisionSpec<'a> {
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub ssh_authorized_keys: &'a [String],
    pub user_data: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
    pub ssh: SshAccess,
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

#[derive(Clone)]
pub struct CompositeWorker<P> {
    provider: P,
    readiness_timeout: Duration,
}

impl<P> CompositeWorker<P> {
    pub fn new(provider: P, readiness_timeout: Duration) -> Self {
        Self {
            provider,
            readiness_timeout,
        }
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
        let machine = self.provider.create(
            &MachineSpec {
                provider_id: ProviderId::parse(spec.backend_id)?,
                disk_id: spec.disk_id,
                memory_mib: spec.memory_mib,
                vcpus: spec.vcpus,
                disk_gib: spec.disk_gib,
                cloud_init: NoCloudConfig {
                    user_data: spec.user_data.to_owned(),
                    vendor_data: vendor_data(&authorized_keys)?,
                },
            },
            log,
        )?;
        let world = inspect_machine(&machine, self.readiness_timeout, log)?;
        verify_login(
            &world.ssh,
            readiness_key.private_key(),
            readiness_key.path(),
        )?;
        remove_readiness_key(
            machine.transport.as_ref(),
            &readiness_key.public_key,
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

pub fn vendor_data(authorized_keys: &[String]) -> Result<String, WorkerError> {
    let keys = authorized_keys
        .iter()
        .map(|key| {
            let mut key = ssh_key::PublicKey::from_openssh(key)
                .map_err(|error| WorkerError::new(format!("parse SSH authorized key: {error}")))?;
            key.set_comment("");
            let key = key
                .to_openssh()
                .map_err(|error| WorkerError::new(format!("encode SSH authorized key: {error}")))?;
            Ok(format!("      - '{key}'"))
        })
        .collect::<Result<Vec<_>, WorkerError>>()?
        .join("\n");
    Ok(format!(
        "#cloud-config\nusers:\n  - name: wt\n    gecos: WT\n    groups: [sudo]\n    shell: /bin/bash\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    lock_passwd: true\n    ssh_authorized_keys:\n{keys}\ndisable_root: true\nssh_pwauth: false\n"
    ))
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
    log: &mut dyn Write,
) -> Result<World, WorkerError> {
    let deadline = Instant::now() + timeout;
    let status = machine.transport.run(
        &RunRequest {
            executable: "/usr/bin/cloud-init",
            args: &["status", "--wait"],
            stdin: None,
            deadline,
        },
        log,
    )?;
    if status.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "cloud-init failed: exit code {}: {}",
            status.exit_code,
            String::from_utf8_lossy(&status.diagnostic_tail).trim()
        )));
    }
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
    })
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

fn remove_readiness_key(
    transport: &dyn GuestTransport,
    public_key: &str,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    let script = "set -eu; file=/home/wt/.ssh/authorized_keys; temporary=$file.wt-readiness; grep -Fvx -- \"$1\" \"$file\" > \"$temporary\"; chown wt:wt \"$temporary\"; chmod 0600 \"$temporary\"; mv -- \"$temporary\" \"$file\"";
    let output = transport.run(
        &RunRequest {
            executable: "/bin/sh",
            args: &["-c", script, "sh", public_key],
            stdin: None,
            deadline,
        },
        log,
    )?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "remove SSH readiness key: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.diagnostic_tail).trim()
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
    fn vendor_data_owns_only_wt_access() {
        insta::assert_snapshot!(vendor_data(&[
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo person's-key".to_owned(),
        ]).unwrap(), @r###"
        #cloud-config
        users:
          - name: wt
            gecos: WT
            groups: [sudo]
            shell: /bin/bash
            sudo: ALL=(ALL) NOPASSWD:ALL
            lock_passwd: true
            ssh_authorized_keys:
              - 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo'
        disable_root: true
        ssh_pwauth: false
        "###);
    }
}
