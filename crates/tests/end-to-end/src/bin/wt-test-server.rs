use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt_kvm::LibvirtProvider;
use wt_retained_worlds::devcontainer::{CompositeWorker, WorldProvisioner};
use wt_retained_worlds::Workers;
use wt_server::config::StateConfig;
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::ServerConfig;
use wt_workload_registry::Store;

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-test-server: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let (config, capacity) = parse_arguments(std::env::args_os().skip(1))?;
    run_api(&config, &capacity)
}

fn parse_arguments(arguments: impl Iterator<Item = OsString>) -> Result<(PathBuf, PathBuf)> {
    let mut arguments = arguments;
    match (
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
        arguments.next(),
    ) {
        (Some(config_flag), Some(config), Some(capacity_flag), Some(capacity), Some(api), None)
            if config_flag == "--config" && capacity_flag == "--capacity" && api == "api" =>
        {
            Ok((PathBuf::from(config), PathBuf::from(capacity)))
        }
        _ => anyhow::bail!("expected --config PATH --capacity PATH api"),
    }
}

fn run_api(config_path: &Path, capacity_path: &Path) -> Result<()> {
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    let capacity = wt_workload_registry::CapacityConfig::load_from(capacity_path)
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
    let host_worker = wt_retained_worlds::host::CompositeWorker::new(
        host_provider,
        Duration::from_secs(server.guest.recipe_timeout_seconds),
        retained,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = Workers::new(CompositeWorker::new(provider, provisioner), host_worker);
    let gateway_socket = std::env::var_os("WT_AGENT_GIT_TEST_CONTROL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(wt_agent_git_gateway::CONTROL_SOCKET));
    let gateway = wt_agent_git_gateway::ControlClient::new(gateway_socket);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_arguments() {
        let (config, capacity) = parse_arguments(
            [
                "--config",
                "/tmp/server.toml",
                "--capacity",
                "/tmp/capacity.toml",
                "api",
            ]
            .map(OsString::from)
            .into_iter(),
        )
        .unwrap();

        assert_eq!(config, PathBuf::from("/tmp/server.toml"));
        assert_eq!(capacity, PathBuf::from("/tmp/capacity.toml"));
    }

    #[test]
    fn rejects_missing_capacity_argument() {
        let error = parse_arguments(
            ["--config", "/tmp/server.toml", "api"]
                .map(OsString::from)
                .into_iter(),
        )
        .unwrap_err();

        insta::assert_snapshot!(error.to_string(), @"expected --config PATH --capacity PATH api");
    }
}
