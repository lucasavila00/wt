use anyhow::{Context, Result};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt_kvm::LibvirtProvider;
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
    let server = ServerConfig::load_runtime_from(config_path).map_err(anyhow::Error::msg)?;
    let test_server = server.test_server;
    let provider = LibvirtProvider::new(server.machine_config()).map_err(anyhow::Error::msg)?;
    let retained = server.retained_config();
    let worker = wt_retained_worlds::host::Worker::new(
        provider,
        Duration::from_secs(server.guest.readiness_timeout_seconds),
        retained,
    )
    .map_err(anyhow::Error::msg)?;
    let gateway_socket = std::env::var_os("WT_AGENT_TOOL_TEST_CONTROL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(wt_agent_tool_gateway::CONTROL_SOCKET));
    let gateway = wt_agent_tool_gateway::ControlClient::new(gateway_socket);
    let service =
        Service::with_capacity_limit(store, worker, gateway, Operations::default(), capacity);
    let response = match serde_json::from_reader::<_, ApiRequest>(std::io::stdin().lock()) {
        Ok(request) => wt_server::handle_request_with_progress(
            &service,
            "lucas",
            request,
            test_server,
            &mut std::io::stderr().lock(),
        ),
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
