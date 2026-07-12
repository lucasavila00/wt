use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::io::{Read, Write};
use uuid::Uuid;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, GitPassphrase, InstanceStatus};
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_server::config::StateConfig;
use wt_server::jobs::{run_provision, Jobs, ProcessLauncher};
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
    /// Handle one JSON request on stdin and write one JSON response to stdout.
    Api,
    #[command(hide = true)]
    Provision(ProvisionArgs),
}

#[derive(Debug, Args)]
struct ProvisionArgs {
    #[arg(long)]
    id: Uuid,
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
        Command::Provision(args) => run_provision_command(args),
    }
}

fn run_api() -> Result<()> {
    let config = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&config.database_path()).context("open instance registry")?;
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server_config.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let jobs = Jobs::open(config.jobs_dir()).context("open provisioning jobs")?;
    let launcher = ProcessLauncher::server().context("configure provisioning launcher")?;
    let owner = process_user()?;
    let mut service = Service::new(store, worker, jobs, launcher);

    let stdin = std::io::stdin();
    let response = match serde_json::from_reader::<_, ApiRequest>(stdin.lock()) {
        Ok(request) => wt_server::handle_request(&mut service, &owner, request),
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, &response)?;
    output.write_all(b"\n")?;
    Ok(())
}

fn run_provision_command(args: ProvisionArgs) -> Result<()> {
    let mut encoded_secret = Vec::new();
    std::io::stdin().read_to_end(&mut encoded_secret)?;
    let passphrase: GitPassphrase =
        serde_json::from_slice(&encoded_secret).context("read Git passphrase")?;
    encoded_secret.fill(0);

    let config = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&config.database_path()).context("open instance registry")?;
    let stored = store.get_by_id(args.id).context("load reserved instance")?;
    if stored.instance.status != InstanceStatus::Provisioning {
        anyhow::bail!("reserved instance is not provisioning");
    }
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server_config.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    run_provision(&store, &worker, stored, &passphrase)
        .map_err(anyhow::Error::msg)
        .context("run provisioning job")
}

fn process_user() -> Result<String> {
    let uid = Uid::effective();
    User::from_uid(uid)
        .context("look up process user")?
        .map(|user| user.name)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no process user for uid {uid}"))
}
