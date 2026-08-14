mod bootstrap;
mod devcontainer;
mod provisioner;

pub use bootstrap::{BootstrapPolicy, PackageSet, PackageVersions, DEVCONTAINER_CLI_VERSION};
pub use provisioner::{ProvisionerConfig, WorldProvisioner};

use std::io::Write;
use uuid::Uuid;
use wt_api::{AppSshAccess, InstanceName, SshAccess};
use wt_provider::{
    ForkError, ForkMachineSpec, MachineInspection, MachineProvider, MachineSpec, ProviderId,
    WorkerError,
};

#[derive(Clone)]
pub struct ProvisionSpec<'a> {
    pub id: Uuid,
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub owner: &'a str,
    pub name: &'a InstanceName,
    pub source: &'a str,
    pub git_base: &'a str,
    pub git_prefix: &'a str,
    pub git_grant: &'a str,
    pub git_user_name: &'a str,
    pub git_user_email: &'a str,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
    pub ssh_authorized_keys: &'a [String],
}

#[derive(Clone, Debug)]
pub struct ForkSpec<'a> {
    pub source_backend_id: &'a str,
    pub source_disk_id: Uuid,
    pub source_head_disk_id: Uuid,
    pub backend_id: &'a str,
    pub disk_id: Uuid,
    pub memory_mib: u64,
    pub vcpus: u32,
    pub disk_gib: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    pub guest_ip: String,
    pub ssh: SshAccess,
    pub app_ssh: Option<AppSshAccess>,
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
    fn fork(&self, spec: &ForkSpec<'_>, log: &mut dyn Write) -> Result<World, ForkError>;
    fn destroy(&self, backend_id: &str, disk_ids: &[Uuid]) -> Result<(), WorkerError>;
    fn inspect(&self, backend_id: &str) -> Result<WorldInspection, WorkerError>;
    fn start(&self, backend_id: &str) -> Result<World, WorkerError>;
}

#[derive(Clone)]
pub struct CompositeWorker<P> {
    provider: P,
    provisioner: WorldProvisioner,
}

impl<P> CompositeWorker<P> {
    pub fn new(provider: P, provisioner: WorldProvisioner) -> Self {
        Self {
            provider,
            provisioner,
        }
    }
}

impl<P: MachineProvider> WorldWorker for CompositeWorker<P> {
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        let provider_id = ProviderId::parse(spec.backend_id)?;
        let machine = self.provider.create(
            &MachineSpec {
                provider_id: provider_id.clone(),
                disk_id: spec.disk_id,
                memory_mib: spec.memory_mib,
                vcpus: spec.vcpus,
                disk_gib: spec.disk_gib,
            },
            log,
        )?;
        self.provisioner.provision(&machine, spec, log)
    }

    fn fork(&self, spec: &ForkSpec<'_>, log: &mut dyn Write) -> Result<World, ForkError> {
        let machine = self.provider.fork(
            &ForkMachineSpec {
                source_provider_id: ProviderId::parse(spec.source_backend_id)
                    .map_err(ForkError::before_pivot)?,
                source_disk_id: spec.source_disk_id,
                source_head_disk_id: spec.source_head_disk_id,
                machine: MachineSpec {
                    provider_id: ProviderId::parse(spec.backend_id)
                        .map_err(ForkError::before_pivot)?,
                    disk_id: spec.disk_id,
                    memory_mib: spec.memory_mib,
                    vcpus: spec.vcpus,
                    disk_gib: spec.disk_gib,
                },
            },
            log,
        )?;
        self.provisioner
            .inspect(&machine)
            .map_err(ForkError::after_pivot)
    }

    fn destroy(&self, backend_id: &str, disk_ids: &[Uuid]) -> Result<(), WorkerError> {
        self.provider
            .delete(&ProviderId::parse(backend_id)?, disk_ids)
    }

    fn inspect(&self, backend_id: &str) -> Result<WorldInspection, WorkerError> {
        match self.provider.inspect(&ProviderId::parse(backend_id)?)? {
            MachineInspection::Missing => Ok(WorldInspection::Missing),
            MachineInspection::Running(machine) => self
                .provisioner
                .inspect(&machine)
                .map(WorldInspection::Running),
            MachineInspection::Stopped { reason } => Ok(WorldInspection::Stopped { reason }),
        }
    }

    fn start(&self, backend_id: &str) -> Result<World, WorkerError> {
        let machine = self.provider.start(&ProviderId::parse(backend_id)?)?;
        self.provisioner.start(&machine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use wt_provider::{
        CaptureRequest, CapturedOutput, GuestTransport, Machine, RunOutput, RunRequest,
        TransportError, WriteFileRequest,
    };

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

        fn fork(
            &self,
            _spec: &ForkMachineSpec,
            _progress: &mut dyn Write,
        ) -> Result<Machine, ForkError> {
            unreachable!()
        }

        fn inspect(&self, _provider_id: &ProviderId) -> Result<MachineInspection, WorkerError> {
            unreachable!()
        }

        fn start(&self, _provider_id: &ProviderId) -> Result<Machine, WorkerError> {
            unreachable!()
        }

        fn delete(&self, _provider_id: &ProviderId, _disk_ids: &[Uuid]) -> Result<(), WorkerError> {
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
        for name in [
            "app-pane",
            "app-info",
            "app-proxy",
            "agent-git-relay",
            "agent-git-remote",
            "agent-git-cli",
            "ca.crt",
        ] {
            fs::write(temp.path().join(name), name).unwrap();
        }
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
        let packages = PackageSet::provisioner()
            .names()
            .iter()
            .map(|name| ((*name).to_owned(), "1".to_owned()))
            .collect();
        let provisioner = WorldProvisioner::new(ProvisionerConfig {
            app_pane_binary: temp.path().join("app-pane"),
            app_info_binary: temp.path().join("app-info"),
            app_proxy_binary: temp.path().join("app-proxy"),
            agent_git_relay_binary: temp.path().join("agent-git-relay"),
            agent_git_remote_binary: temp.path().join("agent-git-remote"),
            agent_git_cli_binary: temp.path().join("agent-git-cli"),
            registry_cache_url: format!("http://{registry_address}"),
            registry_cache_ca_file: temp.path().join("ca.crt"),
            recipe_timeout: Duration::from_secs(10),
            bootstrap: BootstrapPolicy {
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
        );
        let name = InstanceName::parse("failure").unwrap();
        let spec = ProvisionSpec {
            id: Uuid::new_v4(),
            backend_id: "wt-0123456789abcdef0123456789abcdef",
            disk_id: Uuid::new_v4(),
            owner: "tester",
            name: &name,
            source: "git@example.test:repo.git",
            git_base: "main",
            git_prefix: "wt/",
            git_grant: "test-grant",
            git_user_name: "Test User",
            git_user_email: "test@example.invalid",
            memory_mib: 1024,
            vcpus: 1,
            disk_gib: 8,
            ssh_authorized_keys: &["ssh-ed25519 AAAATEST".to_owned()],
        };
        let mut log = Vec::new();
        let error = worker.provision(&spec, &mut log).unwrap_err();
        assert_eq!(deletes.load(Ordering::SeqCst), 0);
        assert!(error.to_string().contains("expected Ubuntu 24.04 amd64"));
        assert!(!error.to_string().contains("cleanup"));
        insta::assert_snapshot!(String::from_utf8(log).unwrap(), @"Verifying and bootstrapping the guest OS...\n");
    }
}
