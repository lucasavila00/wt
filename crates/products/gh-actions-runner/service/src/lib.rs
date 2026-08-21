use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;
use wt_libvirt_kvm::{MachineProvider, MachineSpec, ProviderId, RunRequest};
use wt_workload_registry::{Registry, Resources, Runner, RunnerStatus};
use zeroize::Zeroizing;

const START_RUNNER: &str = r#"set -eu
IFS= read -r jit_config
unset ACTIONS_RUNNER_INPUT_JITCONFIG || true
exec /opt/actions-runner/run.sh --jitconfig "$jit_config"
"#;

pub struct JitConfig {
    pub runner_id: u64,
    encoded: Zeroizing<String>,
}

impl JitConfig {
    pub fn new(runner_id: u64, encoded: impl Into<String>) -> Self {
        Self {
            runner_id,
            encoded: Zeroizing::new(encoded.into()),
        }
    }
}

pub trait JitProvider {
    fn generate(&self, runner_name: &str) -> Result<JitConfig, String>;
}

pub trait RunnerBackend {
    fn run(
        &self,
        runner: &Runner,
        jit: &JitConfig,
        log: &mut dyn Write,
        timeout: Duration,
    ) -> Result<(), String>;

    fn destroy(&self, runner: &Runner) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct LifecycleConfig {
    pub database_path: PathBuf,
    pub log_dir: PathBuf,
    pub runner_resources: Resources,
    pub capacity_limit: Resources,
    pub job_timeout: Duration,
}

impl LifecycleConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, path) in [
            ("database_path", &self.database_path),
            ("log_dir", &self.log_dir),
        ] {
            if !path.is_absolute() {
                return Err(format!("{name} must be an absolute path"));
            }
        }
        if self.runner_resources.vcpus == 0
            || self.runner_resources.memory_mib == 0
            || self.runner_resources.disk_gib == 0
        {
            return Err("runner resources must be greater than zero".into());
        }
        if self.job_timeout.is_zero() {
            return Err("job timeout must be greater than zero".into());
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("runner registry: {0}")]
    Registry(String),
    #[error("create runner log: {0}")]
    Log(String),
    #[error("generate JIT configuration: {0}")]
    Jit(String),
    #[error("run guest runner: {0}")]
    Run(String),
    #[error("runner cleanup: {0}")]
    Cleanup(String),
}

pub struct RunnerManager<J, B> {
    registry: Registry,
    jit: J,
    backend: B,
    config: LifecycleConfig,
}

impl<J: JitProvider, B: RunnerBackend> RunnerManager<J, B> {
    pub fn new(config: LifecycleConfig, jit: J, backend: B) -> Result<Self, LifecycleError> {
        config.validate().map_err(LifecycleError::Registry)?;
        fs::create_dir_all(&config.log_dir)
            .map_err(|error| LifecycleError::Log(error.to_string()))?;
        let registry = Registry::open(&config.database_path)
            .map_err(|error| LifecycleError::Registry(error.to_string()))?;
        Ok(Self {
            registry,
            jit,
            backend,
            config,
        })
    }

    pub fn reconcile(&self) -> Result<(), LifecycleError> {
        let runners = self
            .registry
            .list_runners()
            .map_err(|error| LifecycleError::Registry(error.to_string()))?;
        let mut errors = Vec::new();
        for runner in runners {
            match self.backend.destroy(&runner) {
                Ok(()) => {
                    if let Err(error) = self.registry.release_runner(runner.guest.id) {
                        errors.push(format!("{}: release reservation: {error}", runner.name));
                    }
                }
                Err(error) => {
                    let message = format!("startup cleanup: {error}");
                    if let Err(mark) = self.registry.mark_runner(
                        runner.guest.id,
                        RunnerStatus::CleanupPending,
                        runner.github_runner_id,
                        Some(&message),
                    ) {
                        errors.push(format!(
                            "{}: {message}; record cleanup: {mark}",
                            runner.name
                        ));
                    } else {
                        errors.push(format!("{}: {message}", runner.name));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(LifecycleError::Cleanup(errors.join("; ")))
        }
    }

    pub fn run_one(&self) -> Result<(), LifecycleError> {
        let runner = self.reserve_one()?;
        self.run(runner)
    }

    pub fn reserve_one(&self) -> Result<Runner, LifecycleError> {
        let id = Uuid::new_v4();
        let disk_id = Uuid::new_v4();
        let name = format!("wt-runner-{}", &id.simple().to_string()[..12]);
        self.registry
            .reserve_runner(
                id,
                &name,
                format!("wt-{}", id.simple()),
                disk_id,
                self.config.runner_resources,
                self.config.capacity_limit,
            )
            .map_err(|error| LifecycleError::Registry(error.to_string()))
    }

    pub fn run(&self, runner: Runner) -> Result<(), LifecycleError> {
        let id = runner.guest.id;
        let primary = self.run_reserved(&runner);
        let primary_message = primary.as_ref().err().map(ToString::to_string);
        self.registry
            .mark_runner(
                id,
                RunnerStatus::CleanupPending,
                self.runner_id(id).ok().flatten(),
                primary_message.as_deref(),
            )
            .map_err(|error| LifecycleError::Registry(error.to_string()))?;

        if let Err(cleanup) = self.backend.destroy(&runner) {
            let message = match primary_message {
                Some(primary) => format!("{primary}; cleanup: {cleanup}"),
                None => cleanup,
            };
            self.registry
                .mark_runner(
                    id,
                    RunnerStatus::CleanupPending,
                    self.runner_id(id).ok().flatten(),
                    Some(&message),
                )
                .map_err(|error| LifecycleError::Registry(error.to_string()))?;
            return Err(LifecycleError::Cleanup(message));
        }
        self.registry
            .release_runner(id)
            .map_err(|error| LifecycleError::Registry(error.to_string()))?;
        primary
    }

    fn run_reserved(&self, runner: &Runner) -> Result<(), LifecycleError> {
        let mut log = open_log(&self.config.log_dir, &runner.name)?;
        writeln!(log, "runner {} reserved", runner.name)
            .map_err(|error| LifecycleError::Log(error.to_string()))?;
        let jit = self
            .jit
            .generate(&runner.name)
            .map_err(LifecycleError::Jit)?;
        self.registry
            .mark_runner(
                runner.guest.id,
                RunnerStatus::Starting,
                Some(jit.runner_id),
                None,
            )
            .map_err(|error| LifecycleError::Registry(error.to_string()))?;
        self.backend
            .run(runner, &jit, &mut log, self.config.job_timeout)
            .map_err(LifecycleError::Run)
    }

    fn runner_id(&self, id: Uuid) -> Result<Option<u64>, LifecycleError> {
        self.registry
            .list_runners()
            .map_err(|error| LifecycleError::Registry(error.to_string()))?
            .into_iter()
            .find(|runner| runner.guest.id == id)
            .map(|runner| runner.github_runner_id)
            .ok_or_else(|| LifecycleError::Registry("runner disappeared".into()))
    }
}

fn open_log(log_dir: &Path, name: &str) -> Result<std::fs::File, LifecycleError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(log_dir.join(format!("{name}.log")))
        .map_err(|error| LifecycleError::Log(error.to_string()))
}

#[derive(Clone)]
pub struct LibvirtRunnerBackend<P> {
    provider: P,
}

impl<P> LibvirtRunnerBackend<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

impl<P: MachineProvider> RunnerBackend for LibvirtRunnerBackend<P> {
    fn run(
        &self,
        runner: &Runner,
        jit: &JitConfig,
        log: &mut dyn Write,
        timeout: Duration,
    ) -> Result<(), String> {
        let provider_id =
            ProviderId::parse(&runner.guest.backend_id).map_err(|error| error.to_string())?;
        let machine = self
            .provider
            .create(
                &MachineSpec {
                    provider_id,
                    disk_id: runner.guest.disk_id,
                    memory_mib: runner.guest.resources.memory_mib,
                    vcpus: u32::try_from(runner.guest.resources.vcpus)
                        .map_err(|_| "runner vcpus exceed u32".to_owned())?,
                    disk_gib: runner.guest.resources.disk_gib,
                    cloud_init: wt_libvirt_kvm::NoCloudConfig::default(),
                },
                log,
            )
            .map_err(|error| error.to_string())?;
        let result = machine
            .transport
            .run(
                &RunRequest {
                    executable: "/bin/sh",
                    args: &["-c", START_RUNNER],
                    stdin: Some(jit.encoded.as_bytes()),
                    deadline: Instant::now() + timeout,
                },
                log,
            )
            .map_err(|error| error.to_string())?;
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(format!(
                "runner exited with code {}: {}",
                result.exit_code,
                String::from_utf8_lossy(&result.diagnostic_tail).trim()
            ))
        }
    }

    fn destroy(&self, runner: &Runner) -> Result<(), String> {
        self.provider
            .delete(
                &ProviderId::parse(&runner.guest.backend_id).map_err(|error| error.to_string())?,
                runner.guest.disk_id,
            )
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeJit;

    impl JitProvider for FakeJit {
        fn generate(&self, _runner_name: &str) -> Result<JitConfig, String> {
            Ok(JitConfig::new(42, "fixture-secret-jit"))
        }
    }

    #[derive(Clone, Default)]
    struct FakeBackend {
        state: Arc<Mutex<FakeState>>,
    }

    #[derive(Default)]
    struct FakeState {
        runs: usize,
        destroys: usize,
        run_error: Option<String>,
        destroy_error: Option<String>,
    }

    impl RunnerBackend for FakeBackend {
        fn run(
            &self,
            _runner: &Runner,
            _jit: &JitConfig,
            log: &mut dyn Write,
            _timeout: Duration,
        ) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            state.runs += 1;
            writeln!(log, "runner output").unwrap();
            state.run_error.clone().map_or(Ok(()), Err)
        }

        fn destroy(&self, _runner: &Runner) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            state.destroys += 1;
            state.destroy_error.clone().map_or(Ok(()), Err)
        }
    }

    fn config(temp: &tempfile::TempDir) -> LifecycleConfig {
        LifecycleConfig {
            database_path: temp.path().join("registry.db"),
            log_dir: temp.path().join("logs"),
            runner_resources: Resources {
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
            },
            capacity_limit: Resources {
                vcpus: 16,
                memory_mib: 32768,
                disk_gib: 512,
            },
            job_timeout: Duration::from_secs(60),
        }
    }

    #[test]
    fn successful_runner_is_destroyed_and_released() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::default();
        let manager = RunnerManager::new(config(&temp), FakeJit, backend.clone()).unwrap();

        manager.run_one().unwrap();

        assert!(Registry::open(&config(&temp).database_path)
            .unwrap()
            .list_runners()
            .unwrap()
            .is_empty());
        let state = backend.state.lock().unwrap();
        assert_eq!(state.runs, 1);
        assert_eq!(state.destroys, 1);
        let log = fs::read_to_string(
            fs::read_dir(&config(&temp).log_dir)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        assert!(!log.contains("fixture-secret-jit"));
    }

    #[test]
    fn failed_runner_is_still_destroyed_and_released() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::default();
        backend.state.lock().unwrap().run_error = Some("runner lost".into());
        let manager = RunnerManager::new(config(&temp), FakeJit, backend.clone()).unwrap();

        assert!(matches!(manager.run_one(), Err(LifecycleError::Run(_))));
        assert!(Registry::open(&config(&temp).database_path)
            .unwrap()
            .list_runners()
            .unwrap()
            .is_empty());
        assert_eq!(backend.state.lock().unwrap().destroys, 1);
    }

    #[test]
    fn cleanup_failure_keeps_runner_and_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let backend = FakeBackend::default();
        backend.state.lock().unwrap().destroy_error = Some("libvirt unavailable".into());
        let manager = RunnerManager::new(config(&temp), FakeJit, backend).unwrap();

        assert!(matches!(manager.run_one(), Err(LifecycleError::Cleanup(_))));
        let runners = Registry::open(&config(&temp).database_path)
            .unwrap()
            .list_runners()
            .unwrap();
        assert_eq!(runners[0].status, RunnerStatus::CleanupPending);
        assert!(runners[0]
            .last_error
            .as_deref()
            .unwrap()
            .contains("libvirt unavailable"));
    }

    #[test]
    fn startup_reconciliation_destroys_retained_runners() {
        let temp = tempfile::tempdir().unwrap();
        let config = config(&temp);
        let registry = Registry::open(&config.database_path).unwrap();
        let id = Uuid::new_v4();
        registry
            .reserve_runner(
                id,
                "retained",
                format!("wt-{}", id.simple()),
                Uuid::new_v4(),
                config.runner_resources,
                config.capacity_limit,
            )
            .unwrap();
        let backend = FakeBackend::default();
        let manager = RunnerManager::new(config.clone(), FakeJit, backend.clone()).unwrap();

        manager.reconcile().unwrap();

        assert!(Registry::open(&config.database_path)
            .unwrap()
            .list_runners()
            .unwrap()
            .is_empty());
        assert_eq!(backend.state.lock().unwrap().destroys, 1);
    }
}
pub mod config;
