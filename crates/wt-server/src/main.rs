use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::io::Write;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_server::config::StateConfig;
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
    }
}

fn run_api() -> Result<()> {
    let config = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&config.database_path()).context("open instance registry")?;
    let server_config = ServerConfig::load().map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server_config.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let owner = process_user()?;
    let mut service = Service::new(store, worker);

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

fn process_user() -> Result<String> {
    let uid = Uid::effective();
    User::from_uid(uid)
        .context("look up process user")?
        .map(|user| user.name)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no process user for uid {uid}"))
}
