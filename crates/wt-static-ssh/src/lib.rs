use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use ssh_key::PrivateKey;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use wt_api::{AppSshAccess, GitPassphrase, SshAccess};
use wt_provider::{ProvisionSpec, WorkerError, World, WorldWorker};

const CLAIM: &str = "/var/lib/wt/claim.json";
const BOOTSTRAP: &str = r#"set -eu
if [ "$(uname -m)" != x86_64 ]; then echo 'unsupported architecture; expected x86_64' >&2; exit 1; fi
. /etc/os-release
if [ "$ID" != ubuntu ] || [ "${VERSION_ID:-}" != 24.04 ]; then echo 'unsupported operating system; expected Ubuntu 24.04' >&2; exit 1; fi
if [ "$(id -u)" -ne 0 ]; then sudo -n true; SUDO='sudo -n'; else SUDO=; fi
if [ -e /var/lib/wt/claim.json ] || [ -e /workspace ]; then echo 'static SSH VM is already claimed or has a workspace' >&2; exit 1; fi
if command -v docker >/dev/null 2>&1; then
  test -z "$($SUDO docker ps -aq)" || { echo 'static SSH VM has existing containers' >&2; exit 1; }
  test -z "$($SUDO docker network ls --filter type=custom -q)" || { echo 'static SSH VM has existing user-defined networks' >&2; exit 1; }
  test -z "$($SUDO docker volume ls -q)" || { echo 'static SSH VM has existing named volumes' >&2; exit 1; }
fi
available=$(df --output=avail -BG / | tail -1 | tr -dc '0-9')
test "${available:-0}" -ge "$1" || { echo "insufficient free disk; need ${1} GiB" >&2; exit 1; }
$SUDO install -d -m 0755 /var/lib/wt
tmp=/var/lib/wt/.claim.$$
printf '%s' "$2" | base64 -d | $SUDO tee "$tmp" >/dev/null
$SUDO chmod 0644 "$tmp"
$SUDO ln "$tmp" /var/lib/wt/claim.json || { $SUDO rm -f "$tmp"; echo 'static SSH VM was claimed concurrently' >&2; exit 1; }
$SUDO rm -f "$tmp"
$SUDO apt-get update
DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y docker.io docker-buildx docker-compose-v2 git openssh-server nodejs npm tmux byobu ca-certificates
$SUDO systemctl enable --now docker.service ssh.service
$SUDO npm install -g @devcontainers/cli
$SUDO id wt >/dev/null 2>&1 || $SUDO useradd --create-home --groups docker --shell /bin/bash wt
$SUDO usermod -aG docker wt
$SUDO install -d -m 0700 -o wt -g wt /home/wt/.ssh
printf '%s' "$3" | base64 -d | $SUDO tee /home/wt/.ssh/authorized_keys >/dev/null
$SUDO chown wt:wt /home/wt/.ssh/authorized_keys
$SUDO chmod 0600 /home/wt/.ssh/authorized_keys
$SUDO install -d -m 0755 -o wt -g wt /workspace
"#;

const PROVISION: &str = r#"set -eu
if [ "$(id -u)" -ne 0 ]; then SUDO='sudo -n'; else SUDO=; fi
claim=$(cat /var/lib/wt/claim.json)
test "$claim" = "$(printf '%s' "$1" | base64 -d)" || { echo 'WT claim mismatch' >&2; exit 1; }
$SUDO install -d -m 0700 -o wt -g wt /run/wt-git
printf '%s' "$3" | base64 -d | $SUDO tee /run/wt-git/identity >/dev/null
printf '%s' "$4" | base64 -d | $SUDO tee /run/wt-git/known_hosts >/dev/null
printf '%s' "$5" | base64 -d | $SUDO tee /run/wt-git/passphrase >/dev/null
$SUDO tee /run/wt-git/askpass >/dev/null <<'EOF'
#!/bin/sh
cat /run/wt-git/passphrase
EOF
$SUDO chmod 0600 /run/wt-git/identity /run/wt-git/passphrase
$SUDO chmod 0644 /run/wt-git/known_hosts
$SUDO chmod 0755 /run/wt-git/askpass
$SUDO chown -R wt:wt /run/wt-git
$SUDO -u wt env SSH_ASKPASS=/run/wt-git/askpass SSH_ASKPASS_REQUIRE=force DISPLAY=wt GIT_SSH_COMMAND='ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes' git clone -- "$2" /workspace
$SUDO install -d -m 0755 -o wt -g wt /workspace/.git/wt
$SUDO cp /run/wt-git/identity /workspace/.git/wt/identity
$SUDO cp /run/wt-git/known_hosts /workspace/.git/wt/known_hosts
$SUDO tee /workspace/.git/wt/ssh >/dev/null <<'EOF'
#!/bin/sh
set -eu
d=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
t=$(mktemp)
trap 'rm -f "$t"' EXIT HUP INT TERM
install -m 0600 "$d/identity" "$t"
exec ssh -i "$t" -o IdentitiesOnly=yes -o UserKnownHostsFile="$d/known_hosts" -o StrictHostKeyChecking=yes "$@"
EOF
$SUDO chmod 0444 /workspace/.git/wt/identity /workspace/.git/wt/known_hosts
$SUDO chmod 0555 /workspace/.git/wt/ssh
$SUDO -u wt git -C /workspace config --local core.sshCommand 'sh -c '\''exec "$(git rev-parse --git-common-dir)/wt/ssh" "$@"'\'' wt-ssh'
$SUDO rm -rf /run/wt-git
$SUDO install -d -m 0700 -o wt -g wt /var/lib/wt-app-ssh/public/authorized_keys
$SUDO ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key
$SUDO ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/session_identity
$SUDO chown wt:wt /var/lib/wt-app-ssh/session_identity /var/lib/wt-app-ssh/session_identity.pub
"#;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSshConfig {
    pub host: String,
    pub identity_file: PathBuf,
    pub known_hosts_file: PathBuf,
    pub git_identity_file: PathBuf,
    pub git_known_hosts_file: PathBuf,
    pub ssh_authorized_keys_file: PathBuf,
    pub disk_gib: u64,
    pub session: String,
    pub app_shell_binary: PathBuf,
    pub app_pane_binary: PathBuf,
    pub app_info_binary: PathBuf,
    pub app_proxy_binary: PathBuf,
}

#[derive(Clone)]
pub struct StaticSshWorker {
    config: StaticSshConfig,
    git_identity: Vec<u8>,
    git_known_hosts: Vec<u8>,
    authorized_keys: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct Claim {
    world_id: uuid::Uuid,
    name: String,
}

impl StaticSshWorker {
    pub fn new(config: StaticSshConfig) -> Result<Self, WorkerError> {
        validate_host(&config.host)?;
        if !matches!(config.session.as_str(), "tmux" | "byobu") {
            return Err(WorkerError::new("guest session must be tmux or byobu"));
        }
        for (path, label) in [
            (&config.identity_file, "static SSH identity"),
            (&config.known_hosts_file, "static SSH known-hosts"),
            (&config.git_identity_file, "Git identity"),
            (&config.git_known_hosts_file, "Git known-hosts"),
            (&config.ssh_authorized_keys_file, "guest authorized keys"),
            (&config.app_shell_binary, "app-shell binary"),
            (&config.app_pane_binary, "app-pane binary"),
            (&config.app_info_binary, "app-info binary"),
            (&config.app_proxy_binary, "app-proxy binary"),
        ] {
            require_file(path, label)?;
        }
        let mode = fs::metadata(&config.identity_file)
            .map_err(err("inspect static SSH identity"))?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            return Err(WorkerError::new(
                "static SSH identity must not be accessible by group or others",
            ));
        }
        Ok(Self {
            git_identity: fs::read(&config.git_identity_file).map_err(err("read Git identity"))?,
            git_known_hosts: fs::read(&config.git_known_hosts_file)
                .map_err(err("read Git known-hosts"))?,
            authorized_keys: fs::read(&config.ssh_authorized_keys_file)
                .map_err(err("read guest authorized keys"))?,
            config,
        })
    }

    fn ssh(&self) -> Command {
        let mut command = Command::new("ssh");
        command
            .args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
            ])
            .arg(format!(
                "IdentityFile={}",
                self.config.identity_file.display()
            ))
            .args(["-o"])
            .arg(format!(
                "UserKnownHostsFile={}",
                self.config.known_hosts_file.display()
            ))
            .arg("--")
            .arg(&self.config.host);
        command
    }

    fn run(
        &self,
        script: &str,
        args: &[String],
        log: &mut dyn Write,
    ) -> Result<Vec<u8>, WorkerError> {
        let mut command = self.ssh();
        command
            .arg("sh")
            .arg("-s")
            .arg("--")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(err("start static SSH command"))?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .map_err(err("send static SSH command"))?;
        let output = child
            .wait_with_output()
            .map_err(err("wait for static SSH command"))?;
        log.write_all(&output.stdout)
            .map_err(err("write provisioning log"))?;
        log.write_all(&output.stderr)
            .map_err(err("write provisioning log"))?;
        if !output.status.success() {
            return Err(WorkerError::new(format!(
                "static SSH command failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    fn capture(&self, script: &str, args: &[String]) -> Result<Vec<u8>, WorkerError> {
        self.run(script, args, &mut std::io::sink())
    }

    pub fn proxy(&self) -> Result<(), WorkerError> {
        let mut command = Command::new("ssh");
        command
            .args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
            ])
            .arg(format!(
                "IdentityFile={}",
                self.config.identity_file.display()
            ))
            .args(["-o"])
            .arg(format!(
                "UserKnownHostsFile={}",
                self.config.known_hosts_file.display()
            ))
            .args(["-W", "localhost:22", "--"])
            .arg(&self.config.host);
        let status = command.status().map_err(err("start static SSH proxy"))?;
        if status.success() {
            Ok(())
        } else {
            Err(WorkerError::new(format!(
                "static SSH proxy exited with {status}"
            )))
        }
    }

    fn claim(&self) -> Result<Option<Claim>, WorkerError> {
        let bytes = self.capture(&format!("if [ -r {CLAIM} ]; then cat {CLAIM}; fi\n"), &[])?;
        if bytes.is_empty() {
            return Ok(None);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| WorkerError::new(format!("decode WT claim: {e}")))
    }
}

impl WorldWorker for StaticSshWorker {
    fn validate_git_passphrase(&self, passphrase: &GitPassphrase) -> Result<(), WorkerError> {
        let encoded = std::str::from_utf8(&self.git_identity)
            .map_err(|e| WorkerError::new(format!("decode Git identity: {e}")))?;
        let key = PrivateKey::from_openssh(encoded)
            .map_err(|e| WorkerError::new(format!("parse Git identity: {e}")))?;
        key.decrypt(passphrase.expose_secret())
            .map(|_| ())
            .map_err(|_| WorkerError::new("Git key passphrase is incorrect"))
    }

    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        let claim = Claim {
            world_id: spec.id,
            name: spec.name.to_string(),
        };
        let claim_json = serde_json::to_vec(&claim)
            .map_err(|e| WorkerError::new(format!("encode WT claim: {e}")))?;
        self.run(
            BOOTSTRAP,
            &[
                self.config.disk_gib.to_string(),
                BASE64.encode(&claim_json),
                BASE64.encode(&self.authorized_keys),
            ],
            log,
        )?;
        self.run(
            PROVISION,
            &[
                BASE64.encode(&claim_json),
                spec.source.to_owned(),
                BASE64.encode(&self.git_identity),
                BASE64.encode(&self.git_known_hosts),
                BASE64.encode(spec.git_passphrase.expose_secret()),
            ],
            log,
        )?;
        // The remaining devcontainer orchestration is installed as one root-owned script
        // so it is identical on repeated inspection and easy to audit.
        self.install_and_start_app(log)?;
        self.inspect(spec.backend_id)?
            .ok_or_else(|| WorkerError::new("static SSH VM lost its claim after provisioning"))
    }

    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError> {
        let claim = self
            .claim()?
            .ok_or_else(|| WorkerError::new("WT claim is missing"))?;
        if backend_id != format!("wt-{}", claim.world_id.simple()) {
            return Err(WorkerError::new("WT claim mismatch"));
        }
        self.run(r#"set -eu
if [ "$(id -u)" -ne 0 ]; then SUDO='sudo -n'; else SUDO=; fi
containers=$($SUDO docker ps -aq --filter label=devcontainer.local_folder=/workspace)
projects=
for container in $containers; do
  project=$($SUDO docker inspect -f '{{ index .Config.Labels "com.docker.compose.project" }}' "$container")
  case " $projects " in *" $project "*) ;; *) projects="$projects $project";; esac
done
test -z "$containers" || $SUDO docker rm -f $containers
for project in $projects; do
  networks=$($SUDO docker network ls -q --filter "label=com.docker.compose.project=$project")
  test -z "$networks" || $SUDO docker network rm $networks
  volumes=$($SUDO docker volume ls -q --filter "label=com.docker.compose.project=$project")
  test -z "$volumes" || $SUDO docker volume rm $volumes
done
$SUDO rm -rf /workspace /var/lib/wt-app-ssh /usr/local/bin/wt-app-shell /usr/local/bin/wt-app-pane /usr/local/bin/wt-app-info /usr/local/bin/wt-app-proxy
$SUDO rm -f /var/lib/wt/claim.json
"#, &[], &mut std::io::sink())?;
        Ok(())
    }

    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError> {
        let Some(claim) = self.claim()? else {
            return Ok(None);
        };
        if backend_id != format!("wt-{}", claim.world_id.simple()) {
            return Err(WorkerError::new("WT claim mismatch"));
        }
        let host_keys = lines(&self.capture("cat /etc/ssh/ssh_host_*_key.pub\n", &[])?);
        let info = self.capture("/usr/local/bin/wt-app-info\n", &[])?;
        let target: serde_json::Value = serde_json::from_slice(&info)
            .map_err(|e| WorkerError::new(format!("decode app target: {e}")))?;
        let user = target["user"]
            .as_str()
            .ok_or_else(|| WorkerError::new("app target has no user"))?
            .to_owned();
        let app_keys = lines(&self.capture(
            "cat /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub\n",
            &[],
        )?);
        Ok(Some(World {
            guest_ip: self.config.host.clone(),
            ssh: SshAccess {
                user: "wt".into(),
                host: format!("wt-server-proxy:{backend_id}"),
                port: 22,
                host_keys,
            },
            app_ssh: AppSshAccess {
                user,
                port: 2222,
                host_keys: app_keys,
            },
        }))
    }
}

impl StaticSshWorker {
    fn install_and_start_app(&self, log: &mut dyn Write) -> Result<(), WorkerError> {
        for (source, destination) in [
            (&self.config.app_shell_binary, "/usr/local/bin/wt-app-shell"),
            (&self.config.app_pane_binary, "/usr/local/bin/wt-app-pane"),
            (&self.config.app_info_binary, "/usr/local/bin/wt-app-info"),
            (&self.config.app_proxy_binary, "/usr/local/bin/wt-app-proxy"),
        ] {
            let bytes = fs::read(source).map_err(err("read WT guest helper"))?;
            self.run("set -eu\nif [ \"$(id -u)\" -ne 0 ]; then SUDO='sudo -n'; else SUDO=; fi\nprintf '%s' \"$1\" | base64 -d | $SUDO tee \"$2\" >/dev/null\n$SUDO chmod 0755 \"$2\"\n", &[BASE64.encode(bytes), destination.into()], log)?;
        }
        let script = r#"set -eu
if [ "$(id -u)" -ne 0 ]; then SUDO='sudo -n'; else SUDO=; fi
$SUDO tee /var/lib/wt-app-ssh/public/sshd_config >/dev/null <<'EOF'
Port 2222
HostKey /run/wt-app-ssh/ssh_host_ed25519_key
PidFile /run/sshd-wt.pid
AuthorizedKeysFile /run/wt-app-ssh/authorized_keys/%u
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM yes
PermitRootLogin prohibit-password
AllowTcpForwarding yes
Subsystem sftp internal-sftp
EOF
features='{"ghcr.io/devcontainers/features/sshd:1.1.0":{}}'
$SUDO -u wt env HOME=/home/wt devcontainer up --log-level debug --log-format text --workspace-folder /workspace --additional-features "$features" --mount type=bind,source=/var/lib/wt-app-ssh/public,target=/run/wt-app-ssh --mount type=bind,source=/var/lib/wt-app-ssh/public/sshd_config,target=/etc/ssh/sshd_config
info=$(/usr/local/bin/wt-app-info)
user=$(printf '%s' "$info" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>process.stdout.write(JSON.parse(s).user))')
test -n "$user"
cat /home/wt/.ssh/authorized_keys /var/lib/wt-app-ssh/session_identity.pub | $SUDO tee "/var/lib/wt-app-ssh/public/authorized_keys/$user" >/dev/null
$SUDO chmod 0644 "/var/lib/wt-app-ssh/public/authorized_keys/$user"
key=$(awk '{print $1 " " $2}' /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub)
printf 'wt-app %s\n' "$key" | $SUDO tee /var/lib/wt-app-ssh/known_hosts >/dev/null
$SUDO chmod 0644 /var/lib/wt-app-ssh/known_hosts
address=$(printf '%s' "$info" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>process.stdout.write(JSON.parse(s).address))')
$SUDO -u wt ssh -p 2222 -i /var/lib/wt-app-ssh/session_identity -o BatchMode=yes -o IdentitiesOnly=yes -o UserKnownHostsFile=/var/lib/wt-app-ssh/known_hosts -o StrictHostKeyChecking=yes -o HostKeyAlias=wt-app "$user@$address" true
"#;
        self.run("set -eu\nif [ \"$(id -u)\" -ne 0 ]; then SUDO='sudo -n'; else SUDO=; fi\nprintf 'set-option -g default-command /usr/local/bin/wt-app-pane\\n' | $SUDO tee /usr/local/share/wt-tmux.conf >/dev/null\nprintf '%s\\n' \"$1\" | $SUDO tee /usr/local/share/wt-session-frontend >/dev/null\n$SUDO chmod 0644 /usr/local/share/wt-tmux.conf /usr/local/share/wt-session-frontend\n", std::slice::from_ref(&self.config.session), log)?;
        self.run(script, &[], log).map(|_| ())
    }
}

fn validate_host(host: &str) -> Result<(), WorkerError> {
    if host.is_empty()
        || host.starts_with('-')
        || host.bytes().any(|b| b.is_ascii_whitespace() || b == 0)
    {
        Err(WorkerError::new(
            "static SSH host must be one OpenSSH destination without whitespace",
        ))
    } else {
        Ok(())
    }
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
fn err(action: &'static str) -> impl FnOnce(std::io::Error) -> WorkerError {
    move |e| WorkerError::new(format!("{action}: {e}"))
}
fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_one_openssh_destination_only() {
        assert!(validate_host("work-vm").is_ok());
        assert!(validate_host("user@work-vm").is_ok());
        assert!(validate_host("").is_err());
        assert!(validate_host("-oProxyCommand=bad").is_err());
        assert!(validate_host("work-vm command").is_err());
    }

    #[test]
    fn claim_creation_uses_an_atomic_hard_link() {
        assert!(BOOTSTRAP.contains("ln \"$tmp\" /var/lib/wt/claim.json"));
        assert!(!BOOTSTRAP.contains("mv -n"));
    }
}
