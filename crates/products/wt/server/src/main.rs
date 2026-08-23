use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, ErrorCode, InstanceStatus};
use wt_libvirt_kvm::LibvirtProvider;
use wt_server::config::StateConfig;
use wt_server::daemon::{self, CONTROL_SOCKET_PATH};
use wt_server::operations::Operations;
use wt_server::service::Service;
use wt_server::ServerConfig;
use wt_workload_registry::Store;

#[derive(Debug, Parser)]
#[command(name = "wt-server", version = wt_control_protocol::BUILD_DESCRIPTION)]
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
    wt_server::validate_process_identity().map_err(anyhow::Error::msg)?;
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let codex_paths = server_config.codex_paths();
    wt_server::validate_shared_roots(codex_paths).map_err(anyhow::Error::msg)?;
    let owner = wt_server::SERVER_USER.to_owned();
    eprintln!(
        "wt-server starting: {}",
        wt_control_protocol::BuildIdentity::current()
    );
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    store
        .reconcile_interrupted()
        .context("reconcile interrupted operations at startup")?;
    log_codex_catalog_warnings(
        wt_server::service::refresh_codex_session_catalog(&store, Path::new(codex_paths.sessions))
            .map_err(anyhow::Error::msg)?,
    );
    let operations = Operations::default();
    let capacity_limit = wt_workload_registry::CapacityConfig::load()
        .map_err(anyhow::Error::msg)?
        .limits;
    let test_server = server_config.test_server;
    let codex_sessions = codex_paths.sessions;
    let provider =
        LibvirtProvider::new(server_config.machine_config()).map_err(anyhow::Error::msg)?;
    let host_config = server_config.host_config();
    let host_worker = wt_host_world::host::Worker::new(
        provider,
        Duration::from_secs(server_config.guest.readiness_timeout_seconds),
        host_config,
    )
    .map_err(anyhow::Error::msg)?;
    let catalog_database = state.database_path();
    let catalog_sessions = codex_paths.sessions;
    let catalog_owner = owner.clone();
    let catalog_worker = host_worker.clone();
    std::thread::Builder::new()
        .name("wt-codex-session-catalog".to_owned())
        .spawn(move || {
            let mut requested = HashMap::new();
            loop {
                match maintain_codex_history(
                    &catalog_database,
                    catalog_sessions,
                    &catalog_owner,
                    &catalog_worker,
                    &mut requested,
                ) {
                    Ok(warnings) => log_codex_catalog_warnings(warnings),
                    Err(error) => eprintln!("wt-server: refresh Codex session catalog: {error}"),
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .context("start Codex session catalog refresh")?;
    let worker = host_worker;
    let gateway = wt_agent_tool_gateway::ControlClient::new(wt_agent_tool_gateway::CONTROL_SOCKET);
    let context = DaemonContext {
        state,
        operations,
        worker,
        gateway,
        owner,
        capacity_limit,
        test_server,
        codex_sessions,
    };

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request, progress| {
        handle_daemon_request(&context, request, progress)
    })
}

fn maintain_codex_history(
    database: &Path,
    sessions: &str,
    owner: &str,
    worker: &wt_host_world::host::Worker<LibvirtProvider>,
    requested: &mut HashMap<String, String>,
) -> Result<Vec<String>> {
    let store = Store::open(database).context("open instance registry")?;
    let warnings = wt_server::service::refresh_codex_session_catalog(&store, Path::new(sessions))
        .map_err(anyhow::Error::msg)?;
    let generation =
        wt_server::service::codex_session_catalog_generation(&store).map_err(anyhow::Error::msg)?;
    let worlds = store.list(owner).context("list worlds")?;
    let running = worlds
        .iter()
        .filter(|world| world.instance.status == InstanceStatus::Running)
        .map(|world| world.backend_id.clone())
        .collect::<HashSet<_>>();
    requested.retain(|backend_id, _| running.contains(backend_id));
    for backend_id in running {
        if requested.get(&backend_id) == Some(&generation) {
            continue;
        }
        match worker.request_codex_reconciliation(&backend_id, &generation) {
            Ok(true) => {
                requested.insert(backend_id, generation.clone());
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!("wt-server: request Codex reconciliation in {backend_id}: {error}");
            }
        }
    }
    Ok(warnings)
}

fn log_codex_catalog_warnings(warnings: Vec<String>) {
    for warning in warnings {
        eprintln!("wt-server: Codex session discovery: {warning}");
    }
}

struct DaemonContext {
    state: StateConfig,
    operations: Operations,
    worker: wt_host_world::host::Worker<LibvirtProvider>,
    gateway: wt_agent_tool_gateway::ControlClient,
    owner: String,
    capacity_limit: wt_workload_registry::Resources,
    test_server: bool,
    codex_sessions: &'static str,
}

fn handle_daemon_request(
    context: &DaemonContext,
    request: ApiRequest,
    progress: &mut dyn std::io::Write,
) -> ApiResponse {
    let result = (|| {
        let store =
            Store::open(&context.state.database_path()).context("open instance registry")?;
        let service = Service::with_capacity_limit(
            store,
            context.worker.clone(),
            context.gateway.clone(),
            context.operations.clone(),
            context.capacity_limit,
        )
        .with_codex_sessions_path(context.codex_sessions);
        Ok::<_, anyhow::Error>(wt_server::handle_request_with_progress(
            &service,
            &context.owner,
            request,
            context.test_server,
            progress,
        ))
    })();
    result.unwrap_or_else(request_initialization_error)
}

fn request_initialization_error(error: anyhow::Error) -> ApiResponse {
    eprintln!("wt-server: initialize request: {error:#}");
    ApiResponse::error(ApiError::new(
        ErrorCode::Internal,
        format!("initialize request: {error:#}"),
    ))
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

    #[test]
    fn request_initialization_errors_remain_internal_responses() {
        let response = request_initialization_error(anyhow::anyhow!("database is locked"));
        let wt_control_protocol::Outcome::Error { error } = response.outcome else {
            panic!("initialization failure must return an error response");
        };

        assert_eq!(error.code, ErrorCode::Internal);
        insta::assert_snapshot!(error.message, @"initialize request: database is locked");
    }
}
