use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_devcontainer::{CompositeWorker, WorldProvisioner};
use wt_libvirt::LibvirtProvider;
use wt_server::config::StateConfig;
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::store::Store;
use wt_server::worlds::Workers;
use wt_server::ServerConfig;

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-test-server: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let config = match (arguments.next(), arguments.next()) {
        (Some(flag), Some(path)) if flag == "--config" => PathBuf::from(path),
        _ => anyhow::bail!("expected --config PATH"),
    };
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("api") if arguments.next().is_none() => run_api(&config),
        _ => anyhow::bail!("expected api"),
    }
}

fn run_api(config_path: &Path) -> Result<()> {
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    let capacity = wt_registry::CapacityConfig::load()
        .map_err(anyhow::Error::msg)?
        .limits;
    let server = ServerConfig::load_from(config_path).map_err(anyhow::Error::msg)?;
    let provider =
        LibvirtProvider::new(server.devcontainer_machine_config()).map_err(anyhow::Error::msg)?;
    let host_provider =
        LibvirtProvider::new(server.host_machine_config()).map_err(anyhow::Error::msg)?;
    let registry_cache_url = format!(
        "http://{}:{}",
        provider
            .network_bridge_address()
            .map_err(anyhow::Error::msg)?,
        server.registry_cache.port
    );
    let retained = server.retained_config();
    let provisioner = WorldProvisioner::new(
        server
            .provisioner_config(registry_cache_url, retained.clone())
            .map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    let host_worker = wt_host::CompositeWorker::new(
        host_provider,
        Duration::from_secs(server.guest.recipe_timeout_seconds),
        retained,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = Workers::new(CompositeWorker::new(provider, provisioner), host_worker);
    let gateway_socket = std::env::var_os("WT_AGENT_GIT_TEST_CONTROL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(wt_agent_git::CONTROL_SOCKET));
    let gateway = wt_agent_git::ControlClient::new(gateway_socket);
    let service =
        Service::with_capacity_limit(store, worker, gateway, Operations::default(), capacity);
    let response = match serde_json::from_reader::<_, ApiRequest>(std::io::stdin().lock()) {
        Ok(request) => wt_server::handle_request(&service, "lucas", request),
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    serde_json::to_writer(std::io::stdout().lock(), &response)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
