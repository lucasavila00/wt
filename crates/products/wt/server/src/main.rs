use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::path::Path;
use std::time::Duration;
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt_kvm::LibvirtProvider;
use wt_server::config::StateConfig;
use wt_server::daemon::{self, CONTROL_SOCKET_PATH};
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::ServerConfig;
use wt_workload_registry::Store;

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
    let config = ServerConfig::load_from(Path::new(wt_server::SERVER_CONFIG_PATH))
        .map_err(anyhow::Error::msg)?;
    reject_remote_test_server(
        config.test_server,
        std::env::var_os("SSH_CONNECTION").is_some(),
    )?;
    daemon::proxy(
        Path::new(CONTROL_SOCKET_PATH),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
}

fn reject_remote_test_server(test_server: bool, open_ssh: bool) -> Result<()> {
    if test_server && open_ssh {
        anyhow::bail!("WT test server refuses remote OpenSSH clients");
    }
    Ok(())
}

fn run_server() -> Result<()> {
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    store
        .reconcile_interrupted()
        .context("reconcile interrupted operations at startup")?;
    let operations = Operations::default();
    let capacity_limit = wt_workload_registry::CapacityConfig::load()
        .map_err(anyhow::Error::msg)?
        .limits;
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let test_server = server_config.test_server;
    let provider =
        LibvirtProvider::new(server_config.machine_config()).map_err(anyhow::Error::msg)?;
    let retained = server_config.retained_config();
    let host_worker = wt_retained_worlds::host::Worker::new(
        provider,
        Duration::from_secs(server_config.guest.readiness_timeout_seconds),
        retained,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = host_worker;
    let gateway = wt_agent_tool_gateway::ControlClient::new(wt_agent_tool_gateway::CONTROL_SOCKET);
    let owner = process_user()?;

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request, progress| {
        handle_daemon_request(
            &state,
            &operations,
            &worker,
            &gateway,
            &owner,
            capacity_limit,
            test_server,
            (request, progress),
        )
    })
}

fn handle_daemon_request(
    state: &StateConfig,
    operations: &Operations,
    worker: &wt_retained_worlds::host::Worker<LibvirtProvider>,
    gateway: &wt_agent_tool_gateway::ControlClient,
    owner: &str,
    capacity_limit: wt_workload_registry::Resources,
    test_server: bool,
    request: (ApiRequest, &mut dyn std::io::Write),
) -> ApiResponse {
    let (request, progress) = request;
    let result = (|| {
        let store = Store::open(&state.database_path()).context("open instance registry")?;
        let service = Service::with_capacity_limit(
            store,
            worker.clone(),
            gateway.clone(),
            operations.clone(),
            capacity_limit,
        );
        Ok::<_, anyhow::Error>(wt_server::handle_request_with_progress(
            &service,
            owner,
            request,
            test_server,
            progress,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_clients_are_rejected_only_by_test_servers() {
        assert!(reject_remote_test_server(false, false).is_ok());
        assert!(reject_remote_test_server(false, true).is_ok());
        assert!(reject_remote_test_server(true, false).is_ok());
        insta::assert_snapshot!(
            reject_remote_test_server(true, true).unwrap_err().to_string(),
            @"WT test server refuses remote OpenSSH clients"
        );
    }
}
