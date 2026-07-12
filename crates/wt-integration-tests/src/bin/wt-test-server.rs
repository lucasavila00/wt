use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use wt_api::{
    ApiError, ApiRequest, ApiResponse, ErrorCode, GitPassphrase, InstanceStatus, SshAccess,
};
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_server::config::StateConfig;
use wt_server::jobs::{run_provision, Jobs, ProcessLauncher};
use wt_server::service::Service;
use wt_server::store::Store;

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-test-server: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("fake-provision") => {
            let id = parse_id(&mut arguments)?;
            return run_fake_provision(id, true);
        }
        Some("fake-interrupted") => {
            let id = parse_id(&mut arguments)?;
            return run_fake_provision(id, false);
        }
        _ => {}
    }
    let mut arguments = std::env::args_os().skip(1);
    let config = match (arguments.next(), arguments.next()) {
        (Some(flag), Some(path)) if flag == "--config" => PathBuf::from(path),
        _ => anyhow::bail!("expected --config PATH"),
    };
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("api") if arguments.next().is_none() => run_api(&config),
        Some("provision") => {
            let id = parse_id(&mut arguments)?;
            run_provision_command(&config, id)
        }
        _ => anyhow::bail!("expected api or provision"),
    }
}

fn parse_id(arguments: &mut impl Iterator<Item = std::ffi::OsString>) -> Result<Uuid> {
    let id = match (arguments.next(), arguments.next()) {
        (Some(flag), Some(id)) if flag == "--id" => id
            .to_str()
            .context("instance id is not UTF-8")?
            .parse::<Uuid>()
            .context("parse instance id")?,
        _ => anyhow::bail!("expected --id UUID"),
    };
    if arguments.next().is_some() {
        anyhow::bail!("unexpected arguments");
    }
    Ok(id)
}

fn run_api(config_path: &Path) -> Result<()> {
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    let server = ServerConfig::load_from(config_path).map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let jobs = Jobs::open(state.jobs_dir()).context("open provisioning jobs")?;
    let launcher = ProcessLauncher::new(
        std::env::current_exe()?,
        vec![
            "--config".to_owned(),
            config_path.to_string_lossy().into_owned(),
            "provision".to_owned(),
        ],
    );
    let mut service = Service::new(store, worker, jobs, launcher);
    let response = match serde_json::from_reader::<_, ApiRequest>(std::io::stdin().lock()) {
        Ok(request) => wt_server::handle_request(&mut service, "lucas", request),
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    serde_json::to_writer(std::io::stdout().lock(), &response)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}

fn run_provision_command(config_path: &Path, id: Uuid) -> Result<()> {
    let mut encoded_secret = Vec::new();
    std::io::stdin().read_to_end(&mut encoded_secret)?;
    let passphrase: GitPassphrase =
        serde_json::from_slice(&encoded_secret).context("read Git passphrase")?;
    encoded_secret.fill(0);
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    let stored = store.get_by_id(id).context("load reserved instance")?;
    if stored.instance.status != InstanceStatus::Provisioning {
        anyhow::bail!("reserved instance is not provisioning");
    }
    let server = ServerConfig::load_from(config_path).map_err(anyhow::Error::msg)?;
    let worker = LibvirtWorker::new(server.worker_config().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    run_provision(&store, &worker, stored, &passphrase)
        .map_err(anyhow::Error::msg)
        .context("run provisioning job")
}

fn run_fake_provision(id: Uuid, finish: bool) -> Result<()> {
    let mut encoded_secret = Vec::new();
    std::io::stdin().read_to_end(&mut encoded_secret)?;
    let _: GitPassphrase =
        serde_json::from_slice(&encoded_secret).context("read Git passphrase")?;
    encoded_secret.fill(0);
    let state = StateConfig::from_env().map_err(anyhow::Error::msg)?;
    let store = Store::open(&state.database_path()).context("open instance registry")?;
    store.acknowledge_job(id)?;
    store.append_log(id, b"first chunk\n")?;
    if !finish {
        return Ok(());
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    store.append_log(id, b"second chunk\n")?;
    store.finish_running(
        id,
        "192.0.2.2",
        &SshAccess {
            user: "wt".to_owned(),
            host: "192.0.2.2".to_owned(),
            port: 22,
            host_keys: vec!["ssh-ed25519 AAAATEST guest".to_owned()],
        },
        b"SUCCESS: fake world is running\n",
    )?;
    Ok(())
}
