use crate::bootstrap::{BootstrapPolicy, SessionFrontend};
use crate::devcontainer;
use crate::git;
use crate::{
    CaptureRequest, CapturedOutput, GuestTransport, Machine, ProvisionSpec, RunRequest,
    WorkerError, World, WriteFileRequest,
};
use serde::Deserialize;
use ssh_key::{HashAlg, PublicKey};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wt_command::cmd;

const CAPTURE_LIMIT: usize = 1024 * 1024;
const GUEST_INSTALL: &[u8] = include_bytes!("../../../assets/install-guest.sh");
const GUEST_INSTALL_STAGE: &str = "/tmp/wt-install-guest";

#[derive(Clone, Debug)]
pub struct ProvisionerConfig {
    pub app_shell_binary: PathBuf,
    pub app_pane_binary: PathBuf,
    pub app_info_binary: PathBuf,
    pub app_proxy_binary: PathBuf,
    pub registry_cache_url: String,
    pub registry_cache_ca_file: PathBuf,
    pub git_identity_file: PathBuf,
    pub git_known_hosts_file: PathBuf,
    pub recipe_timeout: Duration,
    pub ssh_authorized_keys: Vec<String>,
    pub session: SessionFrontend,
    pub bootstrap: BootstrapPolicy,
}

#[derive(Clone)]
pub struct WorldProvisioner {
    config: ProvisionerConfig,
    app_shell: Vec<u8>,
    app_pane: Vec<u8>,
    app_info: Vec<u8>,
    app_proxy: Vec<u8>,
    git_credentials: git::Credentials,
    registry_cache_ca: Vec<u8>,
}

#[derive(Deserialize)]
struct AppTarget {
    user: String,
    address: String,
}

impl WorldProvisioner {
    pub fn new(config: ProvisionerConfig) -> Result<Self, WorkerError> {
        config.bootstrap.validate().map_err(WorkerError::new)?;
        verify_registry_cache(&config.registry_cache_url)?;
        let app_shell = require_and_read(&config.app_shell_binary, "guest app-shell binary")?;
        let app_pane = require_and_read(&config.app_pane_binary, "guest app-pane binary")?;
        let app_info = require_and_read(&config.app_info_binary, "guest app-info binary")?;
        let app_proxy = require_and_read(&config.app_proxy_binary, "guest app-proxy binary")?;
        let registry_cache_ca = require_and_read(
            &config.registry_cache_ca_file,
            "registry cache certificate authority",
        )?;
        let git_credentials =
            git::load_credentials(&config.git_identity_file, &config.git_known_hosts_file)?;
        Ok(Self {
            config,
            app_shell,
            app_pane,
            app_info,
            app_proxy,
            git_credentials,
            registry_cache_ca,
        })
    }

    pub fn validate_git_passphrase(
        &self,
        passphrase: &wt_api::GitPassphrase,
    ) -> Result<(), WorkerError> {
        self.git_credentials.validate_passphrase(passphrase)
    }

    pub fn provision(
        &self,
        machine: &Machine,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        wt_api::validate_ssh_git_source(spec.source)
            .map_err(|error| WorkerError::new(format!("Git source: {error}")))?;
        let deadline = Instant::now() + self.config.recipe_timeout;
        let transport = machine.transport.as_ref();
        self.bootstrap(transport, deadline, log)?;

        log_line(log, &format!("Cloning {}...", spec.source))?;
        let phase_started = Instant::now();
        let clone_required = self.prepare_workspace(transport, spec.source, deadline, log)?;
        let checkout = spec
            .git_branch
            .map(git::Checkout::Branch)
            .or_else(|| spec.git_ref.map(git::Checkout::Ref));
        git::install_workspace(
            transport,
            spec.source,
            clone_required,
            checkout,
            &self.git_credentials,
            spec.git_passphrase,
            spec.git_user_name,
            spec.git_user_email,
            deadline,
            log,
        )?;
        report_phase(log, "Git clone and checkout", phase_started)?;

        log_line(log, "Starting the repository devcontainer...")?;
        let phase_started = Instant::now();
        guest::run_phase(
            transport,
            "workspace ownership",
            "/bin/chown",
            &["-R", "wt:wt", "/workspace"],
            deadline,
            log,
        )?;
        let additional_features = format!(r#"{{"{}":{{}}}}"#, devcontainer::SSHD_FEATURE);
        let app_ssh_mount = format!(
            "type=bind,source={},target={}",
            devcontainer::APP_SSH_PUBLIC_DIR,
            devcontainer::APP_SSH_MOUNT
        );
        let sshd_config_mount = format!(
            "type=bind,source={}/sshd_config,target=/etc/ssh/sshd_config",
            devcontainer::APP_SSH_PUBLIC_DIR
        );
        guest::run_phase(
            transport,
            "devcontainer up",
            "/usr/sbin/runuser",
            &[
                "-u",
                "wt",
                "--",
                "/usr/bin/env",
                "HOME=/home/wt",
                "/usr/local/bin/devcontainer",
                "up",
                "--log-level",
                "debug",
                "--log-format",
                "text",
                "--workspace-folder",
                "/workspace",
                "--additional-features",
                &additional_features,
                "--mount",
                &app_ssh_mount,
                "--mount",
                &sshd_config_mount,
            ],
            deadline,
            log,
        )?;
        report_phase(log, "devcontainer up", phase_started)?;

        let phase_started = Instant::now();
        let app_target = self.read_app_target(transport, deadline)?;
        let app_host_keys =
            self.configure_and_verify_app_ssh(transport, &app_target, deadline, log)?;
        guest::run_phase(
            transport,
            "devcontainer Git workspace",
            "/usr/local/bin/devcontainer",
            &[
                "exec",
                "--workspace-folder",
                "/workspace",
                "/bin/sh",
                "-c",
                "workspace=$(pwd -P) && git config --global --add safe.directory \"$workspace\"",
            ],
            deadline,
            log,
        )?;
        report_phase(
            log,
            "app SSH and devcontainer Git verification",
            phase_started,
        )?;

        let host_keys = self.read_host_keys(transport, deadline)?;
        self.verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
        log_line(log, &format!("World {} is ready.", spec.name))?;
        Ok(World {
            guest_ip: machine.guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: machine.guest_ip.clone(),
                port: 22,
                host_keys,
            },
            app_ssh: wt_api::AppSshAccess {
                user: app_target.user,
                port: devcontainer::APP_SSH_PORT,
                host_keys: app_host_keys,
            },
        })
    }

    pub fn inspect(&self, machine: &Machine) -> Result<World, WorkerError> {
        let deadline = Instant::now() + self.config.recipe_timeout;
        let transport = machine.transport.as_ref();
        let host_keys = self.read_host_keys(transport, deadline)?;
        self.verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
        let app_ssh = self.inspect_app_ssh(transport, deadline)?;
        Ok(World {
            guest_ip: machine.guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: machine.guest_ip.clone(),
                port: 22,
                host_keys,
            },
            app_ssh,
        })
    }

    fn bootstrap(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        log_line(log, "Verifying and bootstrapping the guest OS...")?;
        let os = guest::capture_phase(
            transport,
            "guest operating system",
            "/bin/sh",
            &[
                "-c",
                ". /etc/os-release && printf '%s\\n%s\\n' \"$ID\" \"$VERSION_ID\" && uname -m",
            ],
            deadline,
        )?;
        if os != b"ubuntu\n24.04\nx86_64\n" && os != b"ubuntu\n24.04\namd64\n" {
            return Err(WorkerError::new(format!(
                "guest operating system: expected Ubuntu 24.04 amd64, got {}",
                String::from_utf8_lossy(&os).trim()
            )));
        }
        let uid = guest::capture_phase(
            transport,
            "guest privilege",
            "/usr/bin/id",
            &["-u"],
            deadline,
        )?;
        if uid != b"0\n" {
            guest::run_phase(
                transport,
                "passwordless sudo",
                "/usr/bin/sudo",
                &["-n", "/usr/bin/true"],
                deadline,
                log,
            )?;
            return Err(WorkerError::new(
                "guest transport must execute privileged commands as root",
            ));
        }

        let mut authorized_keys = self.config.ssh_authorized_keys.join("\n").into_bytes();
        authorized_keys.push(b'\n');
        for (suffix, contents) in [
            ("-authorized-keys", authorized_keys.as_slice()),
            ("-registry-ca", self.registry_cache_ca.as_slice()),
            ("-app-shell", self.app_shell.as_slice()),
            ("-app-pane", self.app_pane.as_slice()),
            ("-app-info", self.app_info.as_slice()),
            ("-app-proxy", self.app_proxy.as_slice()),
        ] {
            guest::write(
                transport,
                &format!("{GUEST_INSTALL_STAGE}{suffix}"),
                contents,
            )?;
        }

        let packages = self.config.bootstrap.pinned_packages();
        let session = match self.config.session {
            SessionFrontend::Tmux => "tmux",
            SessionFrontend::Byobu => "byobu",
        };
        let mut args = vec![
            &self.config.bootstrap.devcontainer_cli_version,
            session,
            &self.config.registry_cache_url,
        ];
        args.extend(packages.iter().map(String::as_str));
        let result = guest::run_script(
            transport,
            "guest installation",
            GUEST_INSTALL,
            &args,
            deadline,
            log,
        );
        let _ = guest::exec(
            transport,
            "/bin/rm",
            &[
                "-f",
                "/tmp/wt-install-guest-authorized-keys",
                "/tmp/wt-install-guest-registry-ca",
                "/tmp/wt-install-guest-app-shell",
                "/tmp/wt-install-guest-app-pane",
                "/tmp/wt-install-guest-app-info",
                "/tmp/wt-install-guest-app-proxy",
            ],
            deadline,
        );
        result
    }

    fn prepare_workspace(
        &self,
        transport: &dyn GuestTransport,
        source: &str,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<bool, WorkerError> {
        let output = guest::exec(
            transport,
            "/bin/sh",
            &[
                "-c",
                "if test -d /workspace/.git; then git -C /workspace remote get-url origin; elif test -z \"$(find /workspace -mindepth 1 -maxdepth 1 -print -quit)\"; then exit 3; else exit 4; fi",
            ],
            deadline,
        )?;
        match output.exit_code {
            3 => Ok(true),
            0 if String::from_utf8_lossy(&output.stdout).trim() == source => {
                guest::run_phase(
                    transport,
                    "existing checkout reset",
                    "/usr/bin/git",
                    &["-C", "/workspace", "reset", "--hard", "HEAD"],
                    deadline,
                    log,
                )?;
                Ok(false)
            }
            0 => Err(WorkerError::new(
                "workspace already contains a checkout for a different Git source",
            )),
            _ => Err(WorkerError::new(
                "workspace is not empty and is not a valid Git checkout",
            )),
        }
    }

    fn read_app_target(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
    ) -> Result<AppTarget, WorkerError> {
        let output = guest::capture_phase(
            transport,
            "devcontainer app discovery",
            devcontainer::APP_INFO_PATH,
            &[],
            deadline,
        )?;
        let target: AppTarget = serde_json::from_slice(&output)
            .map_err(|error| context("decode devcontainer app discovery", error))?;
        if target.user.is_empty()
            || !target
                .user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(WorkerError::new(
                "devcontainer app discovery: invalid remote user",
            ));
        }
        target
            .address
            .parse::<IpAddr>()
            .map_err(|error| context("parse devcontainer app address", error))?;
        Ok(target)
    }

    fn configure_and_verify_app_ssh(
        &self,
        transport: &dyn GuestTransport,
        target: &AppTarget,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<Vec<String>, WorkerError> {
        let session_public = guest::capture_phase(
            transport,
            "app session public key",
            "/bin/cat",
            &["/var/lib/wt-app-ssh/session_identity.pub"],
            deadline,
        )?;
        let mut authorized_keys = self.config.ssh_authorized_keys.join("\n").into_bytes();
        authorized_keys.push(b'\n');
        authorized_keys.extend_from_slice(&session_public);
        if !authorized_keys.ends_with(b"\n") {
            authorized_keys.push(b'\n');
        }
        let authorized_path = format!(
            "{}/authorized_keys/{}",
            devcontainer::APP_SSH_PUBLIC_DIR,
            target.user
        );
        guest::write_owned(
            transport,
            &authorized_path,
            &authorized_keys,
            "root",
            "root",
            0o644,
            deadline,
        )?;
        let expected = self.read_app_host_keys(transport, deadline)?;
        let scanned = guest::capture_phase(
            transport,
            "app SSH readiness",
            "/usr/bin/ssh-keyscan",
            &[
                "-T",
                "5",
                "-p",
                &devcontainer::APP_SSH_PORT.to_string(),
                &target.address,
            ],
            deadline,
        )?;
        if !host_keys_match(&expected, &String::from_utf8_lossy(&scanned)) {
            return Err(WorkerError::new(
                "app SSH readiness: presented host keys do not match the per-world identity",
            ));
        }
        let known_hosts = normalized_host_keys(&expected.join("\n"))
            .into_iter()
            .map(|key| format!("wt-app {key}\n"))
            .collect::<String>();
        guest::write_owned(
            transport,
            "/var/lib/wt-app-ssh/known_hosts",
            known_hosts.as_bytes(),
            "root",
            "root",
            0o644,
            deadline,
        )?;
        guest::run_phase(
            transport,
            "app SSH authentication",
            "/usr/bin/ssh",
            &[
                "-p",
                &devcontainer::APP_SSH_PORT.to_string(),
                "-i",
                "/var/lib/wt-app-ssh/session_identity",
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "UserKnownHostsFile=/var/lib/wt-app-ssh/known_hosts",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "HostKeyAlias=wt-app",
                &format!("{}@{}", target.user, target.address),
                "true",
            ],
            deadline,
            log,
        )?;
        Ok(expected)
    }

    fn inspect_app_ssh(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
    ) -> Result<wt_api::AppSshAccess, WorkerError> {
        let target = self.read_app_target(transport, deadline)?;
        let expected = self.read_app_host_keys(transport, deadline)?;
        let scanned = guest::capture_phase(
            transport,
            "app SSH readiness",
            "/usr/bin/ssh-keyscan",
            &[
                "-T",
                "5",
                "-p",
                &devcontainer::APP_SSH_PORT.to_string(),
                &target.address,
            ],
            deadline,
        )?;
        if !host_keys_match(&expected, &String::from_utf8_lossy(&scanned)) {
            return Err(WorkerError::new("app SSH host identity changed"));
        }
        Ok(wt_api::AppSshAccess {
            user: target.user,
            port: devcontainer::APP_SSH_PORT,
            host_keys: expected,
        })
    }

    fn read_app_host_keys(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
    ) -> Result<Vec<String>, WorkerError> {
        let bytes = guest::capture_phase(
            transport,
            "app SSH host keys",
            "/bin/cat",
            &["/var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub"],
            deadline,
        )?;
        let keys = String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return Err(WorkerError::new("app SSH host keys: no public keys"));
        }
        Ok(keys)
    }

    fn read_host_keys(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
    ) -> Result<Vec<String>, WorkerError> {
        let output = guest::capture_phase(
            transport,
            "SSH host keys",
            "/bin/sh",
            &["-c", "cat /etc/ssh/ssh_host_*_key.pub"],
            deadline,
        )?;
        let keys = String::from_utf8_lossy(&output)
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
        &self,
        guest_ip: &str,
        expected: &[String],
        deadline: Instant,
    ) -> Result<(), WorkerError> {
        let address: SocketAddr = format!("{guest_ip}:22")
            .parse()
            .map_err(|error| context("parse guest SSH address", error))?;
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(
                    "SSH readiness: timed out waiting for port 22",
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        let output = cmd!("/usr/bin/ssh-keyscan", "-T", "5", "-p", "22", guest_ip)
            .output()
            .map_err(|error| context("scan guest SSH host keys", error))?;
        let presented = String::from_utf8_lossy(&output.stdout);
        if host_keys_match(expected, &presented) {
            Ok(())
        } else {
            Err(endpoint_identity_error(guest_ip, expected, &presented))
        }
    }
}

pub(crate) mod guest {
    use super::*;

    pub(crate) fn run_script(
        transport: &dyn GuestTransport,
        phase: &str,
        script: &[u8],
        args: &[&str],
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        let mut shell_args = vec!["-s", "--"];
        shell_args.extend_from_slice(args);
        run_request(
            transport,
            phase,
            "/bin/sh",
            &shell_args,
            Some(script),
            deadline,
            log,
        )
    }

    pub(crate) fn run_phase(
        transport: &dyn GuestTransport,
        phase: &str,
        executable: &str,
        args: &[&str],
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        run_request(transport, phase, executable, args, None, deadline, log)
    }

    fn run_request(
        transport: &dyn GuestTransport,
        phase: &str,
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
            .map_err(|error| WorkerError::new(format!("{phase}: {error}")))?;
        if output.exit_code != 0 {
            return Err(WorkerError::new(format!(
                "{phase}: exit code {}: {}",
                output.exit_code,
                String::from_utf8_lossy(&output.diagnostic_tail).trim()
            )));
        }
        Ok(())
    }

    pub(crate) fn capture_phase(
        transport: &dyn GuestTransport,
        phase: &str,
        executable: &str,
        args: &[&str],
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkerError> {
        let output = exec(transport, executable, args, deadline)
            .map_err(|error| WorkerError::new(format!("{phase}: {error}")))?;
        if output.exit_code != 0 {
            let tail = tail_output(&output.stdout, &output.stderr);
            return Err(WorkerError::new(format!(
                "{phase}: exit code {}: {tail}",
                output.exit_code
            )));
        }
        Ok(output.stdout)
    }

    pub(crate) fn exec(
        transport: &dyn GuestTransport,
        executable: &str,
        args: &[&str],
        deadline: Instant,
    ) -> Result<CapturedOutput, WorkerError> {
        transport
            .capture(&CaptureRequest {
                executable,
                args,
                stdin: None,
                deadline,
                stdout_limit: CAPTURE_LIMIT,
                stderr_limit: CAPTURE_LIMIT,
            })
            .map_err(WorkerError::from)
    }

    pub(crate) fn write(
        transport: &dyn GuestTransport,
        path: &str,
        contents: &[u8],
    ) -> Result<(), WorkerError> {
        write_owned(
            transport,
            path,
            contents,
            "root",
            "root",
            0o600,
            Instant::now() + Duration::from_secs(60),
        )
    }

    pub(crate) fn write_owned(
        transport: &dyn GuestTransport,
        path: &str,
        contents: &[u8],
        owner: &str,
        group: &str,
        mode: u32,
        deadline: Instant,
    ) -> Result<(), WorkerError> {
        transport
            .write_file(&WriteFileRequest {
                path,
                contents,
                owner,
                group,
                mode,
                deadline,
            })
            .map_err(WorkerError::from)
    }

    fn tail_output(stdout: &[u8], stderr: &[u8]) -> String {
        let mut combined = Vec::with_capacity(stdout.len() + stderr.len() + 1);
        combined.extend_from_slice(stdout);
        if !stdout.is_empty() && !stderr.is_empty() {
            combined.push(b'\n');
        }
        combined.extend_from_slice(stderr);
        let start = combined.len().saturating_sub(64 * 1024);
        String::from_utf8_lossy(&combined[start..])
            .trim()
            .to_owned()
    }
}

fn normalized_host_keys(lines: &str) -> BTreeSet<String> {
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

fn is_host_key_kind(value: &str) -> bool {
    value.starts_with("ssh-") || value.starts_with("ecdsa-") || value.starts_with("sk-")
}

fn host_keys_match(expected: &[String], presented: &str) -> bool {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    !expected.is_disjoint(&presented)
}

fn fingerprints(keys: &BTreeSet<String>) -> String {
    if keys.is_empty() {
        return "none".to_owned();
    }
    keys.iter()
        .map(|key| {
            PublicKey::from_openssh(key)
                .map(|key| key.fingerprint(HashAlg::Sha256).to_string())
                .unwrap_or_else(|_| "invalid-key".to_owned())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn endpoint_identity_error(guest_ip: &str, expected: &[String], presented: &str) -> WorkerError {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    WorkerError::new(format!(
        "SSH endpoint identity mismatch at {guest_ip}:22: expected [{}], presented [{}]. WT refused to publish SSH access because another guest may be using this IP. Inspect the server's DHCP and provider state, remove the stale guest, then run `wt sync`.",
        fingerprints(&expected),
        fingerprints(&presented),
    ))
}

fn require_and_read(path: &PathBuf, label: &str) -> Result<Vec<u8>, WorkerError> {
    if !path.is_file() {
        return Err(WorkerError::new(format!(
            "{label} not found: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| context(&format!("read {label}"), error))
}

fn verify_registry_cache(url: &str) -> Result<(), WorkerError> {
    let output = cmd!(
        "/usr/bin/curl",
        "-fsS",
        "--output",
        "/dev/null",
        format!("{url}/ca.crt")
    )
    .output()
    .map_err(|error| context("verify registry cache", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "verify registry cache: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

fn log_line(log: &mut dyn Write, message: &str) -> Result<(), WorkerError> {
    writeln!(log, "{message}").map_err(|error| context("write provisioning log", error))
}

fn report_phase(log: &mut dyn Write, label: &str, started: Instant) -> Result<(), WorkerError> {
    log_line(
        log,
        &format!("{label} ready in {:.1}s.", started.elapsed().as_secs_f64()),
    )
}
