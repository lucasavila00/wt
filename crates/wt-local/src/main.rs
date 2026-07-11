use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{Uid, User};
use std::io::{Read, Write};
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};
use wt_libvirt::{LibvirtWorker, SiteConfig};
use wt_local::config::LocalConfig;
use wt_local::service::Service;
use wt_local::store::Store;

#[derive(Debug, Parser)]
#[command(name = "wt-local")]
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
        eprintln!("wt-local: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Api => run_api(),
    }
}

fn run_api() -> Result<()> {
    let config = LocalConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&config.database_path()).context("open instance registry")?;
    let site_config = SiteConfig::load().map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(site_config.worker_config()).map_err(anyhow::Error::msg)?;
    let owner = process_user()?;
    let mut service = Service::new(store, worker);

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let response = match serde_json::from_str::<ApiRequest>(&input) {
        Ok(request) => wt_local::handle_request(&mut service, &owner, request),
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
