use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use wt_control_protocol::SshAccess;
use wt_libvirt_kvm::{CaptureRequest, GuestTransport, RunRequest, WorkerError};

pub const WT_IDENTITY: WtIdentity = WtIdentity {
    user: "wt",
    group: "wt",
    uid: 1001,
    gid: 1001,
    home: "/home/wt",
};
pub const GUEST_USER: &str = WT_IDENTITY.user;
pub const GUEST_GROUP: &str = WT_IDENTITY.group;
pub const GUEST_HOME: &str = WT_IDENTITY.home;
pub const GUEST_UID: u32 = WT_IDENTITY.uid;
pub const GUEST_GID: u32 = WT_IDENTITY.gid;
pub const GUEST_IDENTITY: GuestIdentity = WT_IDENTITY.numeric();
pub const GUEST_SSH_PORT: u16 = 22;
pub const ACCESS_HELPER: &str = "/usr/local/libexec/wt-guest-access";
pub const GIT_AUTHOR_HELPER: &str = "/usr/local/libexec/wt-guest-git-author";
pub const AGENT_TOOLS_HELPER: &str = "/usr/local/libexec/wt-guest-agent-tools";
pub const MOUNT_CODEX_HELPER: &str = "/usr/local/libexec/wt-guest-mount-codex";

const AGENT_TOOLS_STAGE: &str = "/tmp/wt-guest-agent-tools-";
const GIT_AUTHOR_STAGE: &str = "/tmp/wt-guest-git-author-";
const CAPTURE_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WtIdentity {
    pub user: &'static str,
    pub group: &'static str,
    pub uid: u32,
    pub gid: u32,
    pub home: &'static str,
}

impl WtIdentity {
    pub const fn numeric(self) -> GuestIdentity {
        GuestIdentity {
            uid: self.uid,
            gid: self.gid,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GuestIdentity {
    pub uid: u32,
    pub gid: u32,
}

#[derive(Deserialize)]
struct PinnedImageManifest {
    guest_identity: GuestIdentity,
}

pub fn validate_guest_identity(actual: GuestIdentity) -> Result<(), WorkerError> {
    if actual == GUEST_IDENTITY {
        return Ok(());
    }
    Err(WorkerError::new(format!(
        "guest image guest identity mismatch: expected UID/GID {}:{}, got {}:{}",
        GUEST_IDENTITY.uid, GUEST_IDENTITY.gid, actual.uid, actual.gid
    )))
}

pub fn verify_pinned_image_identity(image: &Path) -> Result<(), WorkerError> {
    let manifest = image_manifest_path(image);
    let bytes = fs::read(&manifest).map_err(|error| {
        WorkerError::new(format!(
            "read pinned image manifest {}: {error}",
            manifest.display()
        ))
    })?;
    let manifest: PinnedImageManifest = serde_json::from_slice(&bytes).map_err(|error| {
        WorkerError::new(format!(
            "parse pinned image manifest {}: {error}",
            manifest.display()
        ))
    })?;
    validate_guest_identity(manifest.guest_identity)
}

pub(crate) fn image_manifest_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.manifest.json", image.display()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestAccess {
    guest_ip: String,
    ssh: SshAccess,
}

impl GuestAccess {
    pub fn from_guest_ip(guest_ip: impl Into<String>, host_keys: Vec<String>) -> Self {
        let guest_ip = guest_ip.into();
        Self {
            ssh: SshAccess {
                user: GUEST_USER.to_owned(),
                host: guest_ip.clone(),
                port: GUEST_SSH_PORT,
                host_keys,
            },
            guest_ip,
        }
    }

    pub fn guest_ip(&self) -> &str {
        &self.guest_ip
    }

    pub fn ssh(&self) -> &SshAccess {
        &self.ssh
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentToolsConfig {
    pub provider_hosts: Vec<String>,
    pub vsock_port: u32,
}

impl AgentToolsConfig {
    pub fn validate(&self) -> Result<(), WorkerError> {
        if self.vsock_port == 0 || self.vsock_port == u32::MAX {
            return Err(WorkerError::new(
                "agent tool vsock port must be concrete and nonzero",
            ));
        }
        if self.provider_hosts.is_empty()
            || self.provider_hosts.iter().any(|host| {
                host.is_empty()
                    || host != &host.to_ascii_lowercase()
                    || host.starts_with('.')
                    || host.ends_with('.')
                    || !host.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'.' | b'-')
                    })
            })
        {
            return Err(WorkerError::new(
                "agent tool provider hosts must be nonempty lowercase DNS names",
            ));
        }
        let hosts = self
            .provider_hosts
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if hosts.len() != self.provider_hosts.len() {
            return Err(WorkerError::new(
                "agent tool provider hosts must not contain duplicates",
            ));
        }
        Ok(())
    }

    pub fn provider_hosts_file(&self) -> String {
        format!("{}\n", self.provider_hosts.join("\n"))
    }

    pub fn vsock_port_file(&self) -> String {
        format!("{}\n", self.vsock_port)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostConfig {
    pub agent_tools: AgentToolsConfig,
}

#[derive(Clone, Copy, Debug)]
pub struct ProvisionSpec<'a> {
    pub authorized_keys: &'a [String],
    pub git_user_name: &'a str,
    pub git_user_email: &'a str,
    pub git_grant: &'a str,
}

impl HostConfig {
    pub fn validate(&self) -> Result<(), WorkerError> {
        self.agent_tools.validate()
    }

    pub fn provision(
        &self,
        transport: &dyn GuestTransport,
        spec: ProvisionSpec<'_>,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        let phase_started = Instant::now();
        self.install_access(transport, spec.authorized_keys, deadline, log)?;
        crate::write_creation_timing(log, "install SSH access", phase_started.elapsed())?;
        let phase_started = Instant::now();
        self.install_git_author(
            transport,
            spec.git_user_name,
            spec.git_user_email,
            deadline,
            log,
        )?;
        crate::write_creation_timing(log, "install Git author", phase_started.elapsed())?;
        let phase_started = Instant::now();
        self.install_agent_tools(transport, spec.git_grant, deadline, log)?;
        crate::write_creation_timing(log, "configure agent tools", phase_started.elapsed())?;
        let phase_started = Instant::now();
        self.mount_codex(transport, deadline, log)?;
        crate::write_creation_timing(log, "mount Codex state", phase_started.elapsed())
    }

    fn install_access(
        &self,
        transport: &dyn GuestTransport,
        authorized_keys: &[String],
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        let contents = authorized_keys_file(authorized_keys)?;
        run_helper(
            transport,
            ACCESS_HELPER,
            &[],
            Some(contents.as_bytes()),
            deadline,
            log,
        )
    }

    fn install_agent_tools(
        &self,
        transport: &dyn GuestTransport,
        grant: &str,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        if grant.is_empty() {
            return Err(WorkerError::new("agent tool grant must not be empty"));
        }
        self.agent_tools.validate()?;
        for (suffix, contents) in [
            ("grant", grant.as_bytes().to_vec()),
            (
                "providers",
                self.agent_tools.provider_hosts_file().into_bytes(),
            ),
            (
                "vsock-port",
                self.agent_tools.vsock_port_file().into_bytes(),
            ),
        ] {
            let path = format!("{AGENT_TOOLS_STAGE}{suffix}");
            transport
                .write_file(&wt_libvirt_kvm::WriteFileRequest {
                    path: &path,
                    contents: &contents,
                    owner: "root",
                    group: "root",
                    mode: 0o600,
                    deadline,
                })
                .map_err(WorkerError::from)?;
        }
        run_helper(transport, AGENT_TOOLS_HELPER, &[], None, deadline, log)
    }

    fn install_git_author(
        &self,
        transport: &dyn GuestTransport,
        name: &str,
        email: &str,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        if name.is_empty() || email.is_empty() {
            return Err(WorkerError::new(
                "Git author name and email must not be empty",
            ));
        }
        for (suffix, contents) in [("name", name.as_bytes()), ("email", email.as_bytes())] {
            transport
                .write_file(&wt_libvirt_kvm::WriteFileRequest {
                    path: &format!("{GIT_AUTHOR_STAGE}{suffix}"),
                    contents,
                    owner: "root",
                    group: "root",
                    mode: 0o600,
                    deadline,
                })
                .map_err(WorkerError::from)?;
        }
        run_helper(transport, GIT_AUTHOR_HELPER, &[], None, deadline, log)
    }

    pub fn mount_codex(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        run_helper(transport, MOUNT_CODEX_HELPER, &[], None, deadline, log)
    }
}

pub fn authorized_keys_file(authorized_keys: &[String]) -> Result<String, WorkerError> {
    let keys = authorized_keys
        .iter()
        .map(|key| {
            let mut key = ssh_key::PublicKey::from_openssh(key)
                .map_err(|error| WorkerError::new(format!("parse SSH authorized key: {error}")))?;
            key.set_comment("");
            key.to_openssh()
                .map_err(|error| WorkerError::new(format!("encode SSH authorized key: {error}")))
        })
        .collect::<Result<Vec<_>, WorkerError>>()?
        .join("\n");
    Ok(format!("{keys}\n"))
}

pub fn normalized_host_keys(lines: &str) -> std::collections::BTreeSet<String> {
    lines
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let first = fields.next()?;
            let (kind, data) = if is_host_key_kind(first) {
                (first, fields.next()?)
            } else {
                (fields.next()?, fields.next()?)
            };
            is_host_key_kind(kind).then(|| format!("{kind} {data}"))
        })
        .collect()
}

pub fn host_keys_match(expected: &[String], presented: &str) -> bool {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    !expected.is_disjoint(&presented)
}

pub fn read_host_keys(
    transport: &dyn GuestTransport,
    deadline: Instant,
) -> Result<Vec<String>, WorkerError> {
    let output = transport
        .capture(&CaptureRequest {
            executable: "/bin/sh",
            args: &["-c", "cat /etc/ssh/ssh_host_*_key.pub"],
            stdin: None,
            deadline,
            stdout_limit: CAPTURE_LIMIT,
            stderr_limit: CAPTURE_LIMIT,
        })
        .map_err(WorkerError::from)?;
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

pub fn verify_guest_ssh(
    guest_ip: &str,
    expected: &[String],
    deadline: Instant,
) -> Result<(), WorkerError> {
    let address: SocketAddr = format!("{guest_ip}:{GUEST_SSH_PORT}")
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
        .args(["-T", "5", "-p", &GUEST_SSH_PORT.to_string(), guest_ip])
        .output()
        .map_err(|error| WorkerError::new(format!("scan guest SSH host keys: {error}")))?;
    let presented = String::from_utf8_lossy(&output.stdout);
    if host_keys_match(expected, &presented) {
        Ok(())
    } else {
        Err(endpoint_identity_error(guest_ip, expected, &presented))
    }
}

pub fn endpoint_identity_error(
    guest_ip: &str,
    expected: &[String],
    presented: &str,
) -> WorkerError {
    fn fingerprints(keys: &std::collections::BTreeSet<String>) -> String {
        if keys.is_empty() {
            return "none".to_owned();
        }
        keys.iter()
            .map(|key| {
                ssh_key::PublicKey::from_openssh(key)
                    .map(|key| key.fingerprint(ssh_key::HashAlg::Sha256).to_string())
                    .unwrap_or_else(|_| "invalid-key".to_owned())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    WorkerError::new(format!(
        "SSH endpoint identity mismatch at {guest_ip}:{GUEST_SSH_PORT}: expected [{}], presented [{}]. WT refused to publish SSH access because another guest may be using this IP. Inspect the server's DHCP and provider state, remove the stale guest, then run `wt sync`.",
        fingerprints(&expected),
        fingerprints(&presented),
    ))
}

fn run_helper(
    transport: &dyn GuestTransport,
    executable: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    let output = transport
        .run(
            &RunRequest {
                executable,
                args,
                stdin,
                deadline,
            },
            log,
        )
        .map_err(WorkerError::from)?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "{executable}: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.diagnostic_tail).trim()
        )));
    }
    Ok(())
}

fn is_host_key_kind(value: &str) -> bool {
    value.starts_with("ssh-") || value.starts_with("ecdsa-") || value.starts_with("sk-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_and_rust_identity_contracts_match() {
        let contract =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../assets/server/wt-identity.sh");
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(
                ". \"$1\"; printf '%s:%s:%s:%s:%s\\n' \"$WT_IDENTITY_USER\" \"$WT_IDENTITY_GROUP\" \"$WT_IDENTITY_UID\" \"$WT_IDENTITY_GID\" \"$WT_IDENTITY_HOME\"",
            )
            .arg("wt-identity-parity")
            .arg(contract)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "{}:{}:{}:{}:{}\n",
                WT_IDENTITY.user,
                WT_IDENTITY.group,
                WT_IDENTITY.uid,
                WT_IDENTITY.gid,
                WT_IDENTITY.home
            )
        );
    }

    #[test]
    fn canonicalizes_authorized_keys() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo person's-key";
        insta::assert_snapshot!(authorized_keys_file(&[key.to_owned()]).unwrap(), @r###"
        ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPAo47CHM4yuzilWsuXWaYMSnEUMOCBQjSTLIofQSNqo
        "###);
    }

    #[test]
    fn normalizes_keyscan_host_prefixes_and_sk_keys() {
        let keys = normalized_host_keys(
            "[192.0.2.1]:22 ssh-ed25519 AAAA\nsk-ssh-ed25519@openssh.com BBBB comment\n",
        );
        assert_eq!(keys.len(), 2);
        assert!(host_keys_match(
            &["ssh-ed25519 AAAA".to_owned()],
            "192.0.2.1 ssh-ed25519 AAAA comment"
        ));
    }

    #[test]
    fn rejects_duplicate_agent_tools_hosts() {
        let config = AgentToolsConfig {
            provider_hosts: vec!["github.com".to_owned(), "github.com".to_owned()],
            vsock_port: 18017,
        };
        let error = config.validate().unwrap_err();
        assert_eq!(
            error.to_string(),
            "agent tool provider hosts must not contain duplicates"
        );
    }
}
