use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::Path;
use std::path::PathBuf;
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
#[command(name = "wts", version = wt_control_protocol::BUILD_DESCRIPTION)]
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

#[allow(dead_code)]
fn main() {
    if let Err(error) = run_from(std::env::args_os()) {
        eprintln!("wt-server: {error:#}");
        std::process::exit(1);
    }
}

pub fn run_from(
    args: impl IntoIterator<Item = impl Into<std::ffi::OsString> + Clone>,
) -> Result<()> {
    match Cli::parse_from(args).command {
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
    wt_server::shared_files::publish_and_watch(codex_paths)?;
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
    let operations = Operations::default();
    let capacity_limit = wt_workload_registry::CapacityConfig::load()
        .map_err(anyhow::Error::msg)?
        .limits;
    let test_server = server_config.test_server;
    let provider =
        LibvirtProvider::new(server_config.machine_config()).map_err(anyhow::Error::msg)?;
    let host_config = server_config.host_config();
    let guest_worker = wt_guest::host::Worker::new(
        provider,
        Duration::from_secs(server_config.guest.readiness_timeout_seconds),
        host_config,
    )
    .map_err(anyhow::Error::msg)?;
    let worker = guest_worker;
    let gateway = open_gateway(&server_config, &state.database_path())?;
    wt_agent_tool_gateway::start_vsock(gateway.clone(), server_config.agent_tools.vsock_port)?;
    let context = DaemonContext {
        state,
        operations,
        worker,
        gateway,
        owner,
        capacity_limit,
        test_server,
    };

    daemon::serve(Path::new(CONTROL_SOCKET_PATH), move |request, progress| {
        handle_daemon_request(&context, request, progress)
    })
}

fn open_gateway(
    config: &ServerConfig,
    database_path: &Path,
) -> Result<wt_agent_tool_gateway::Gateway> {
    let credentials = std::env::var_os("CREDENTIALS_DIRECTORY")
        .map(PathBuf::from)
        .context("CREDENTIALS_DIRECTORY is not set")?;
    let providers = [
        (
            wt_tools::ProviderKind::GitHub,
            "github",
            config.agent_tools.github.as_ref(),
        ),
        (
            wt_tools::ProviderKind::GitLab,
            "gitlab",
            config.agent_tools.gitlab.as_ref(),
        ),
    ]
    .into_iter()
    .filter_map(|(kind, name, provider)| provider.map(|provider| (kind, name, provider)))
    .map(
        |(kind, name, provider)| wt_agent_tool_gateway::Provider::Ssh {
            kind,
            host: provider.host.clone(),
            user: "git".to_owned(),
            port: None,
            api_token_file: credentials.join(format!("{name}-api-token")),
            private_key_file: credentials.join(format!("{name}-ssh-private-key")),
        },
    )
    .collect();
    wt_agent_tool_gateway::Gateway::open(wt_agent_tool_gateway::GatewayConfig {
        state_file: PathBuf::from("/var/lib/wt/agent-tools/state.json"),
        database_path: database_path.to_owned(),
        providers,
    })
}

struct DaemonContext {
    state: StateConfig,
    operations: Operations,
    worker: wt_guest::host::Worker<LibvirtProvider>,
    gateway: wt_agent_tool_gateway::Gateway,
    owner: String,
    capacity_limit: wt_workload_registry::Resources,
    test_server: bool,
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
        );
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
