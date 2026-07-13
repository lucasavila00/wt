mod bootstrap;
mod devcontainer;
mod git;
mod provisioner;
mod transport;

pub use bootstrap::{
    BootstrapPolicy, PackageSet, PackageVersions, SessionFrontend, DEVCONTAINER_CLI_VERSION,
};
pub use provisioner::{ProvisionerConfig, WorldProvisioner};
pub use transport::{
    validate_executable, validate_file_path, CaptureRequest, CapturedOutput, GuestTransport,
    RunOutput, RunRequest, StreamKind, TransportError, WriteFileRequest,
};

use std::fmt;
use std::io::Write;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;
use wt_api::{AppSshAccess, InstanceName, SshAccess};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn parse(value: &str) -> Result<Self, WorkerError> {
        let suffix = value.strip_prefix("wt-").unwrap_or_default();
        if suffix.len() != 32
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(WorkerError::new(
                "provider ID must have the form wt- followed by 32 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSpec {
    pub provider_id: ProviderId,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
}

#[derive(Clone)]
pub struct Machine {
    pub provider_id: ProviderId,
    pub guest_ip: String,
    pub transport: Arc<dyn GuestTransport>,
}

impl fmt::Debug for Machine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Machine")
            .field("provider_id", &self.provider_id)
            .field("guest_ip", &self.guest_ip)
            .field("transport", &"<guest transport>")
            .finish()
    }
}

pub trait MachineProvider: Clone + Send + Sync + 'static {
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError>;
    fn inspect(&self, provider_id: &ProviderId) -> Result<Option<Machine>, WorkerError>;
    fn delete(&self, provider_id: &ProviderId) -> Result<(), WorkerError>;
}

#[derive(Clone, Debug)]
pub struct ProvisionSpec<'a> {
    pub id: Uuid,
    pub backend_id: &'a str,
    pub owner: &'a str,
    pub name: &'a InstanceName,
    pub source: &'a str,
    pub git_branch: Option<&'a str>,
    pub git_ref: Option<&'a str>,
    pub git_passphrase: &'a wt_api::GitPassphrase,
    pub git_user_name: &'a str,
    pub git_user_email: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
    pub ssh: SshAccess,
    pub app_ssh: AppSshAccess,
}

pub trait WorldWorker {
    fn validate_git_passphrase(
        &self,
        passphrase: &wt_api::GitPassphrase,
    ) -> Result<(), WorkerError>;
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError>;
    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError>;
}

#[derive(Clone)]
pub struct CompositeWorker<P> {
    provider: P,
    provisioner: WorldProvisioner,
    machine_resources: MachineResources,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineResources {
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
}

impl<P> CompositeWorker<P> {
    pub fn new(
        provider: P,
        provisioner: WorldProvisioner,
        machine_resources: MachineResources,
    ) -> Self {
        Self {
            provider,
            provisioner,
            machine_resources,
        }
    }
}

impl<P: MachineProvider> WorldWorker for CompositeWorker<P> {
    fn validate_git_passphrase(
        &self,
        passphrase: &wt_api::GitPassphrase,
    ) -> Result<(), WorkerError> {
        self.provisioner.validate_git_passphrase(passphrase)
    }

    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        let provider_id = ProviderId::parse(spec.backend_id)?;
        let machine = self.provider.create(
            &MachineSpec {
                provider_id: provider_id.clone(),
                memory_mib: self.machine_resources.memory_mib,
                vcpus: self.machine_resources.vcpus,
                disk_gib: self.machine_resources.disk_gib,
            },
            log,
        )?;
        match self.provisioner.provision(&machine, spec, log) {
            Ok(world) => Ok(world),
            Err(error) => {
                if let Err(cleanup) = self.provider.delete(&provider_id) {
                    let _ = writeln!(log, "CLEANUP ERROR: {cleanup}");
                }
                Err(error)
            }
        }
    }

    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError> {
        self.provider.delete(&ProviderId::parse(backend_id)?)
    }

    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError> {
        let Some(machine) = self.provider.inspect(&ProviderId::parse(backend_id)?)? else {
            return Ok(None);
        };
        self.provisioner.inspect(&machine).map(Some)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct WorkerError {
    message: String,
}

impl WorkerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<TransportError> for WorkerError {
    fn from(error: TransportError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn provider_ids_are_safe_stable_resource_names() {
        assert!(ProviderId::parse("wt-0123456789abcdef0123456789abcdef").is_ok());
        for invalid in [
            "../wt-0123456789abcdef0123456789abcdef",
            "wt-0123456789ABCDEF0123456789ABCDEF",
            "other-0123456789abcdef0123456789abcdef",
            "wt-short",
        ] {
            assert!(ProviderId::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[derive(Clone)]
    struct FailingProvisionProvider {
        deletes: Arc<AtomicUsize>,
        cleanup_fails: bool,
    }

    impl MachineProvider for FailingProvisionProvider {
        fn create(
            &self,
            spec: &MachineSpec,
            _progress: &mut dyn Write,
        ) -> Result<Machine, WorkerError> {
            Ok(Machine {
                provider_id: spec.provider_id.clone(),
                guest_ip: "192.0.2.2".to_owned(),
                transport: Arc::new(UnsupportedOsTransport),
            })
        }

        fn inspect(&self, _provider_id: &ProviderId) -> Result<Option<Machine>, WorkerError> {
            unreachable!()
        }

        fn delete(&self, _provider_id: &ProviderId) -> Result<(), WorkerError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            if self.cleanup_fails {
                Err(WorkerError::new("injected cleanup failure"))
            } else {
                Ok(())
            }
        }
    }

    struct UnsupportedOsTransport;

    impl GuestTransport for UnsupportedOsTransport {
        fn run(
            &self,
            _request: &RunRequest<'_>,
            _output: &mut dyn Write,
        ) -> Result<RunOutput, TransportError> {
            unreachable!()
        }

        fn capture(&self, _request: &CaptureRequest<'_>) -> Result<CapturedOutput, TransportError> {
            Ok(CapturedOutput {
                exit_code: 0,
                stdout: b"debian\n13\nx86_64\n".to_vec(),
                stderr: Vec::new(),
            })
        }

        fn write_file(&self, _request: &WriteFileRequest<'_>) -> Result<(), TransportError> {
            unreachable!()
        }
    }

    #[test]
    fn provision_failure_keeps_primary_error_and_logs_cleanup_failure() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["app-shell", "app-pane", "app-info", "app-proxy", "ca.crt"] {
            fs::write(temp.path().join(name), name).unwrap();
        }
        let identity = temp.path().join("identity");
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "secret", "-f"])
            .arg(&identity)
            .status()
            .unwrap();
        assert!(status.success());
        fs::set_permissions(&identity, fs::Permissions::from_mode(0o600)).unwrap();
        let known_hosts = temp.path().join("known_hosts");
        fs::write(&known_hosts, "example.test ssh-ed25519 AAAATEST\n").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let registry_address = listener.local_addr().unwrap();
        let registry = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
        });
        let session = SessionFrontend::Tmux;
        let packages = PackageSet::provisioner(session)
            .names()
            .iter()
            .map(|name| ((*name).to_owned(), "1".to_owned()))
            .collect();
        let provisioner = WorldProvisioner::new(ProvisionerConfig {
            app_shell_binary: temp.path().join("app-shell"),
            app_pane_binary: temp.path().join("app-pane"),
            app_info_binary: temp.path().join("app-info"),
            app_proxy_binary: temp.path().join("app-proxy"),
            registry_cache_url: format!("http://{registry_address}"),
            registry_cache_ca_file: temp.path().join("ca.crt"),
            git_identity_file: identity,
            git_known_hosts_file: known_hosts,
            recipe_timeout: Duration::from_secs(10),
            ssh_authorized_keys: vec!["ssh-ed25519 AAAATEST".to_owned()],
            session,
            bootstrap: BootstrapPolicy {
                session,
                packages,
                devcontainer_cli_version: DEVCONTAINER_CLI_VERSION.to_owned(),
            },
        })
        .unwrap();
        registry.join().unwrap();
        let deletes = Arc::new(AtomicUsize::new(0));
        let worker = CompositeWorker::new(
            FailingProvisionProvider {
                deletes: deletes.clone(),
                cleanup_fails: true,
            },
            provisioner,
            MachineResources {
                memory_mib: 1024,
                vcpus: 1,
                disk_gib: 8,
            },
        );
        let name = InstanceName::parse("failure").unwrap();
        let passphrase = wt_api::GitPassphrase::new("secret".to_owned());
        let spec = ProvisionSpec {
            id: Uuid::new_v4(),
            backend_id: "wt-0123456789abcdef0123456789abcdef",
            owner: "tester",
            name: &name,
            source: "git@example.test:repo.git",
            git_branch: None,
            git_ref: None,
            git_passphrase: &passphrase,
            git_user_name: "Test User",
            git_user_email: "test@example.invalid",
        };
        let mut log = Vec::new();
        let error = worker.provision(&spec, &mut log).unwrap_err();
        assert_eq!(deletes.load(Ordering::SeqCst), 1);
        assert!(error.to_string().contains("expected Ubuntu 24.04 amd64"));
        assert!(!error.to_string().contains("cleanup"));
        insta::assert_snapshot!(String::from_utf8(log).unwrap(), @r###"
        Verifying and bootstrapping the guest OS...
        CLEANUP ERROR: injected cleanup failure
        "###);
    }
}
