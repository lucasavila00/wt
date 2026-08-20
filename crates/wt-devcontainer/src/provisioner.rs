mod containers;
mod support;

use crate::bootstrap::BootstrapPolicy;
use crate::devcontainer;
use crate::{ProvisionSpec, World};
use serde::Deserialize;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use support::{context, log_line, require_and_read, verify_registry_cache};
use wt_provider::{
    CaptureRequest, CapturedOutput, GuestTransport, Machine, RunRequest, WorkerError,
    WriteFileRequest,
};
use wt_retained::{GuestAccess, RetainedConfig};

use crate::devcontainer::APP_SSH_PORT;

const CAPTURE_LIMIT: usize = 1024 * 1024;
const GUEST_INSTALL: &[u8] = include_bytes!("../../../assets/world/devcontainer/install-guest.sh");
const SETUP_WORLD: &[u8] = include_bytes!("../../../assets/world/devcontainer/setup-world.sh");
const SETUP_WORLD_ROOT: &[u8] =
    include_bytes!("../../../assets/world/devcontainer/setup-world-root.sh");
const APP_SHELL: &[u8] = include_bytes!("../../../assets/world/devcontainer/app-shell.sh");
const AGENT_GIT_HINT: &[u8] =
    include_bytes!("../../../assets/world/devcontainer/agent-git-hint.sh");
const GUEST_INSTALL_STAGE: &str = "/tmp/wt-install-guest";
const START_READINESS_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct ProvisionerConfig {
    pub app_pane_binary: PathBuf,
    pub app_info_binary: PathBuf,
    pub app_proxy_binary: PathBuf,
    pub registry_cache_url: String,
    pub registry_cache_ca_file: PathBuf,
    pub recipe_timeout: Duration,
    pub bootstrap: BootstrapPolicy,
    pub retained: RetainedConfig,
}

#[derive(Clone)]
pub struct WorldProvisioner {
    config: ProvisionerConfig,
    app_shell: Vec<u8>,
    app_pane: Vec<u8>,
    app_info: Vec<u8>,
    app_proxy: Vec<u8>,
    registry_cache_ca: Vec<u8>,
}

#[derive(Deserialize)]
struct AppTarget {
    user: String,
    address: String,
}

#[derive(Clone, Copy)]
enum InspectionMode {
    Observe,
    RecoverAfterStart,
}

impl WorldProvisioner {
    pub fn new(config: ProvisionerConfig) -> Result<Self, WorkerError> {
        config.bootstrap.validate().map_err(WorkerError::new)?;
        verify_registry_cache(&config.registry_cache_url)?;
        let app_shell = APP_SHELL.to_vec();
        let app_pane = require_and_read(&config.app_pane_binary, "guest app-pane binary")?;
        let app_info = require_and_read(&config.app_info_binary, "guest app-info binary")?;
        let app_proxy = require_and_read(&config.app_proxy_binary, "guest app-proxy binary")?;
        config.retained.validate()?;
        let registry_cache_ca = require_and_read(
            &config.registry_cache_ca_file,
            "registry cache certificate authority",
        )?;
        Ok(Self {
            config,
            app_shell,
            app_pane,
            app_info,
            app_proxy,
            registry_cache_ca,
        })
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
        self.bootstrap(transport, spec, deadline, log)?;

        let host_keys = wt_retained::read_host_keys(transport, deadline)?;
        wt_retained::verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
        log_line(
            log,
            &format!("World {} is ready for setup over SSH.", spec.name),
        )?;
        Ok(World {
            access: GuestAccess::from_guest_ip(machine.guest_ip.clone(), host_keys),
            app_ssh: None,
        })
    }

    pub fn inspect(&self, machine: &Machine) -> Result<World, WorkerError> {
        let deadline = Instant::now() + self.config.recipe_timeout;
        self.inspect_with_deadline(machine, deadline, InspectionMode::Observe)
    }

    fn inspect_with_deadline(
        &self,
        machine: &Machine,
        deadline: Instant,
        mode: InspectionMode,
    ) -> Result<World, WorkerError> {
        let transport = machine.transport.as_ref();
        let host_keys = wt_retained::read_host_keys(transport, deadline)?;
        wt_retained::verify_guest_ssh(&machine.guest_ip, &host_keys, deadline)?;
        let complete = guest::exec(
            transport,
            "/usr/bin/test",
            &["-e", "/var/lib/wt-setup/complete"],
            deadline,
        )?
        .exit_code
            == 0;
        let app_ssh = if complete {
            let target = self.read_app_target(transport, deadline)?;
            if matches!(mode, InspectionMode::RecoverAfterStart) {
                containers::restore_app_access(transport, &target.user, deadline)?;
            }
            let host_keys = self.configure_and_verify_app_ssh(
                transport,
                &target,
                deadline,
                &mut std::io::sink(),
            )?;
            Some(wt_api::AppSshAccess {
                user: target.user,
                port: APP_SSH_PORT,
                host_keys,
            })
        } else {
            None
        };
        Ok(World {
            access: GuestAccess::from_guest_ip(machine.guest_ip.clone(), host_keys),
            app_ssh,
        })
    }

    pub fn start(&self, machine: &Machine) -> Result<World, WorkerError> {
        let deadline = Instant::now() + self.config.recipe_timeout;
        self.config.retained.mount_shared_folders(
            machine.transport.as_ref(),
            deadline,
            &mut std::io::sink(),
        )?;
        containers::start_all(machine.transport.as_ref(), deadline)?;
        loop {
            match self.inspect_with_deadline(machine, deadline, InspectionMode::RecoverAfterStart) {
                Ok(world) => return Ok(world),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(START_READINESS_POLL_INTERVAL);
                }
                Err(error) => {
                    return Err(WorkerError::new(format!(
                        "world readiness after start: {error}"
                    )))
                }
            }
        }
    }

    pub(crate) fn mount_shared_folders_for(
        &self,
        machine: &Machine,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        self.config.retained.mount_shared_folders(
            machine.transport.as_ref(),
            Instant::now() + self.config.recipe_timeout,
            log,
        )
    }

    fn bootstrap(
        &self,
        transport: &dyn GuestTransport,
        spec: &ProvisionSpec<'_>,
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

        for (suffix, contents) in [
            ("-registry-ca", self.registry_cache_ca.as_slice()),
            ("-app-shell", self.app_shell.as_slice()),
            ("-app-pane", self.app_pane.as_slice()),
            ("-app-info", self.app_info.as_slice()),
            ("-app-proxy", self.app_proxy.as_slice()),
            ("-agent-git-hint", AGENT_GIT_HINT),
            ("-setup-world", SETUP_WORLD),
            ("-setup-world-root", SETUP_WORLD_ROOT),
        ] {
            guest::write(
                transport,
                &format!("{GUEST_INSTALL_STAGE}{suffix}"),
                contents,
            )?;
        }
        for (name, contents) in [
            ("source", spec.source),
            ("git-base", spec.git_base),
            ("git-prefix", spec.git_prefix),
            ("git-user-name", spec.git_user_name),
            ("git-user-email", spec.git_user_email),
        ] {
            guest::write(
                transport,
                &format!("/tmp/wt-setup-{name}"),
                contents.as_bytes(),
            )?;
        }
        let packages = self.config.bootstrap.pinned_packages();
        let mut args: Vec<&str> = vec![
            self.config.bootstrap.devcontainer_cli_version.as_str(),
            self.config.registry_cache_url.as_str(),
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
                "/tmp/wt-install-guest-registry-ca",
                "/tmp/wt-install-guest-app-shell",
                "/tmp/wt-install-guest-app-pane",
                "/tmp/wt-install-guest-app-info",
                "/tmp/wt-install-guest-app-proxy",
                "/tmp/wt-install-guest-agent-git-hint",
                "/tmp/wt-install-guest-setup-world",
                "/tmp/wt-install-guest-setup-world-root",
            ],
            deadline,
        );
        result?;
        self.config.retained.provision(
            transport,
            wt_retained::ProvisionSpec {
                authorized_keys: spec.ssh_authorized_keys,
                git_user_name: spec.git_user_name,
                git_user_email: spec.git_user_email,
                git_grant: spec.git_grant,
            },
            deadline,
            log,
        )
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
        if session_public.is_empty() {
            return Err(WorkerError::new("app session public key is empty"));
        }
        let expected = self.read_app_host_keys(transport, deadline)?;
        let scanned = guest::capture_phase(
            transport,
            "app SSH readiness",
            "/usr/bin/ssh-keyscan",
            &["-T", "5", "-p", &APP_SSH_PORT.to_string(), &target.address],
            deadline,
        )?;
        if !wt_retained::host_keys_match(&expected, &String::from_utf8_lossy(&scanned)) {
            return Err(WorkerError::new(
                "app SSH readiness: presented host keys do not match the per-world identity",
            ));
        }
        let known_hosts = wt_retained::normalized_host_keys(&expected.join("\n"))
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
                &APP_SSH_PORT.to_string(),
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
