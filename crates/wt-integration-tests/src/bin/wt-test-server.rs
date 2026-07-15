use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt::LibvirtProvider;
use wt_provider::{CompositeWorker, WorldProvisioner};
use wt_server::config::StateConfig;
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::store::Store;
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
    let server = ServerConfig::load_from(config_path).map_err(anyhow::Error::msg)?;
    let provider = LibvirtProvider::new(server.machine_config()).map_err(anyhow::Error::msg)?;
    let registry_cache_url = format!(
        "http://{}:{}",
        provider
            .network_bridge_address()
            .map_err(anyhow::Error::msg)?,
        server.registry_cache.port
    );
    let provisioner = WorldProvisioner::new(
        server
            .provisioner_config(registry_cache_url)
            .map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = CompositeWorker::new(provider, provisioner, server.machine_resources());
    let service = Service::new(store, worker, Operations::default());
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
