use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_provider::{ProvisionSpec, WorkerError, World, WorldWorker};
use wt_server::config::StateConfig;
use wt_server::daemon::{self, CONTROL_SOCKET_PATH};
use wt_server::jobs::{Jobs, ThreadLauncher};
use wt_server::service::Service;
use wt_server::store::Store;
use wt_static_ssh::{StaticSshConfig, StaticSshWorker};

#[derive(Debug, Parser)]
#[command(name = "wt-server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Forward one JSON request on stdin to the local wt-server daemon.
    Api,
    /// Run the long-lived WT control-plane service.
    Serve,
    /// Proxy stdin/stdout to the configured static SSH machine's SSH port.
    Proxy { world_id: String },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-server: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Api => run_api(),
        Command::Serve => run_server(),
        Command::Proxy { world_id } => run_proxy(&world_id),
    }
}

fn run_api() -> Result<()> {
    daemon::proxy(
        Path::new(CONTROL_SOCKET_PATH),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
}

fn run_server() -> Result<()> {
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    let jobs = Jobs::open(state.jobs_dir()).context("open provisioning jobs")?;
    jobs.reconcile(&store)
        .context("reconcile interrupted jobs at startup")?;
    let worker = ConfiguredWorker::load()?;
    let owner = process_user()?;

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request| {
        handle_daemon_request(&state, &jobs, &worker, &owner, request)
    })
}

fn handle_daemon_request(
    state: &StateConfig,
    jobs: &Jobs,
    worker: &ConfiguredWorker,
    owner: &str,
    request: ApiRequest,
) -> ApiResponse {
    let result = (|| {
        let store = Store::open(&state.database_path()).context("open instance registry")?;
        let mut service = Service::new(store, worker.clone(), jobs.clone(), ThreadLauncher);
        Ok::<_, anyhow::Error>(wt_server::handle_request(&mut service, owner, request))
    })();
    result.unwrap_or_else(|error| {
        ApiResponse::error(ApiError::new(
            ErrorCode::Internal,
            format!("initialize request: {error:#}"),
        ))
    })
}

#[derive(Clone)]
enum ConfiguredWorker {
    Libvirt(Box<LibvirtWorker>),
    Static(Box<StaticSshWorker>),
}

impl ConfiguredWorker {
    fn load() -> Result<Self> {
        let text = std::fs::read_to_string(wt_libvirt::SERVER_CONFIG_PATH)
            .context("read server config")?;
        let kind = toml::from_str::<BackendProbe>(&text)
            .context("parse backend selection")?
            .backend
            .map(|b| b.kind)
            .unwrap_or_else(|| "libvirt".to_owned());
        match kind.as_str() {
            "libvirt" => {
                let config = ServerConfig::load().map_err(anyhow::Error::msg)?;
                Ok(Self::Libvirt(Box::new(
                    LibvirtWorker::new(config.worker_config().map_err(anyhow::Error::msg)?)
                        .map_err(anyhow::Error::msg)?,
                )))
            }
            "static_ssh" => Ok(Self::Static(Box::new(
                StaticSshWorker::new(static_config(&text)?).map_err(anyhow::Error::msg)?,
            ))),
            other => anyhow::bail!("unsupported backend kind {other:?}"),
        }
    }
}

impl WorldWorker for ConfiguredWorker {
    fn validate_git_passphrase(&self, value: &wt_api::GitPassphrase) -> Result<(), WorkerError> {
        match self {
            Self::Libvirt(w) => w.validate_git_passphrase(value),
            Self::Static(w) => w.validate_git_passphrase(value),
        }
    }
    fn provision(
        &self,
        spec: &ProvisionSpec<'_>,
        log: &mut dyn std::io::Write,
    ) -> Result<World, WorkerError> {
        match self {
            Self::Libvirt(w) => w.provision(spec, log),
            Self::Static(w) => w.provision(spec, log),
        }
    }
    fn destroy(&self, id: &str) -> Result<(), WorkerError> {
        match self {
            Self::Libvirt(w) => w.destroy(id),
            Self::Static(w) => w.destroy(id),
        }
    }
    fn inspect(&self, id: &str) -> Result<Option<World>, WorkerError> {
        match self {
            Self::Libvirt(w) => w.inspect(id),
            Self::Static(w) => w.inspect(id),
        }
    }
}

#[derive(Deserialize)]
struct BackendProbe {
    backend: Option<BackendKind>,
}
#[derive(Deserialize)]
struct BackendKind {
    kind: String,
}

#[derive(Deserialize)]
struct StaticRuntime {
    backend: StaticBackend,
    git: StaticGit,
    guest: StaticGuest,
    install: StaticInstall,
}
#[derive(Deserialize)]
struct StaticBackend {
    host: String,
    identity_file: PathBuf,
    known_hosts_file: PathBuf,
}
#[derive(Deserialize)]
struct StaticGit {
    identity_file: PathBuf,
    known_hosts_file: PathBuf,
}
#[derive(Deserialize)]
struct StaticGuest {
    session: String,
    disk_gib: u64,
    ssh_authorized_keys_file: PathBuf,
}
#[derive(Deserialize)]
struct StaticInstall {
    binary_dir: PathBuf,
}

fn static_config(text: &str) -> Result<StaticSshConfig> {
    let value: StaticRuntime = toml::from_str(text).context("parse static SSH server config")?;
    let binary = |name: &str| value.install.binary_dir.join(name);
    Ok(StaticSshConfig {
        host: value.backend.host,
        identity_file: expand_home(value.backend.identity_file)?,
        known_hosts_file: expand_home(value.backend.known_hosts_file)?,
        git_identity_file: expand_home(value.git.identity_file)?,
        git_known_hosts_file: expand_home(value.git.known_hosts_file)?,
        ssh_authorized_keys_file: expand_home(value.guest.ssh_authorized_keys_file)?,
        disk_gib: value.guest.disk_gib,
        session: value.guest.session,
        app_shell_binary: binary("wt-app-shell"),
        app_pane_binary: binary("wt-app-pane"),
        app_info_binary: binary("wt-app-info"),
        app_proxy_binary: binary("wt-app-proxy"),
    })
}

fn expand_home(path: PathBuf) -> Result<PathBuf> {
    if let Ok(rest) = path.strip_prefix("~") {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join(rest))
    } else {
        Ok(path)
    }
}

fn run_proxy(world_id: &str) -> Result<()> {
    let worker = ConfiguredWorker::load()?;
    let ConfiguredWorker::Static(worker) = worker else {
        anyhow::bail!("proxy is only available for static_ssh backends");
    };
    worker
        .inspect(world_id)
        .map_err(anyhow::Error::msg)?
        .context("static SSH world is missing")?;
    worker.proxy().map_err(anyhow::Error::msg)
}

fn process_user() -> Result<String> {
    let uid = Uid::effective();
    User::from_uid(uid)
        .context("look up process user")?
        .map(|user| user.name)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no process user for uid {uid}"))
}
