use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::path::Path;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_server::config::StateConfig;
use wt_server::daemon::{self, CONTROL_SOCKET_PATH};
use wt_server::jobs::{Jobs, ThreadLauncher};
use wt_server::service::Service;
use wt_server::store::Store;

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
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server_config.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let owner = process_user()?;

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request| {
        handle_daemon_request(&state, &jobs, &worker, &owner, request)
    })
}

fn handle_daemon_request(
    state: &StateConfig,
    jobs: &Jobs,
    worker: &LibvirtWorker,
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

fn process_user() -> Result<String> {
    let uid = Uid::effective();
    User::from_uid(uid)
        .context("look up process user")?
        .map(|user| user.name)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no process user for uid {uid}"))
}
