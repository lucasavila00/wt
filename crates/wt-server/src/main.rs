use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::path::Path;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_devcontainer::{CompositeWorker, WorldProvisioner};
use wt_libvirt::LibvirtProvider;
use wt_server::config::StateConfig;
use wt_server::daemon::{self, CONTROL_SOCKET_PATH};
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::store::Store;
use wt_server::ServerConfig;

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
    store
        .reconcile_interrupted()
        .context("reconcile interrupted operations at startup")?;
    let operations = Operations::default();
    let capacity_limit = wt_registry::CapacityConfig::load()
        .map_err(anyhow::Error::msg)?
        .limits;
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let provider =
        LibvirtProvider::new(server_config.machine_config()).map_err(anyhow::Error::msg)?;
    let registry_cache_url = format!(
        "http://{}:{}",
        provider
            .network_bridge_address()
            .map_err(anyhow::Error::msg)?,
        server_config.registry_cache.port
    );
    let provisioner = WorldProvisioner::new(
        server_config
            .provisioner_config(registry_cache_url)
            .map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = CompositeWorker::new(provider, provisioner);
    let gateway = wt_devcontainer_git::ControlClient::new(wt_devcontainer_git::CONTROL_SOCKET);
    let owner = process_user()?;

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request| {
        handle_daemon_request(
            &state,
            &operations,
            &worker,
            &gateway,
            &owner,
            capacity_limit,
            request,
        )
    })
}

fn handle_daemon_request(
    state: &StateConfig,
    operations: &Operations,
    worker: &CompositeWorker<LibvirtProvider>,
    gateway: &wt_devcontainer_git::ControlClient,
    owner: &str,
    capacity_limit: wt_registry::Resources,
    request: ApiRequest,
) -> ApiResponse {
    let result = (|| {
        let store = Store::open(&state.database_path()).context("open instance registry")?;
        let service = Service::with_capacity_limit(
            store,
            worker.clone(),
            gateway.clone(),
            operations.clone(),
            capacity_limit,
        );
        Ok::<_, anyhow::Error>(wt_server::handle_request(&service, owner, request))
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
