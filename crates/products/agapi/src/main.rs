mod api;
mod codex;
mod store;

use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

#[derive(Parser)]
#[command(version, about = "Agent API; independent of its execution environment")]
struct Cli {
    #[arg(long, env = "AGAPI_STATE_DIR")]
    state_dir: PathBuf,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Read one versioned JSON request from stdin and write one JSON response.
    Api,
    /// Supervise the Codex adapter and reconcile its durable result outbox.
    Serve {
        #[arg(long, env = "AGAPI_CODEX", default_value = "codex")]
        codex: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Action::Api => {
            let mut input = String::new();
            io::stdin()
                .take(1024 * 1024 + 1)
                .read_to_string(&mut input)?;
            ensure!(input.len() <= 1024 * 1024, "request exceeds 1 MiB");
            let input = serde_json::from_str(&input)?;
            let response = match api::execute(&cli.state_dir, &input) {
                Ok(result) => json!({"api_version":1, "request_id":input["request_id"],
                    "outcome":"ok", "result":result}),
                Err(error) => json!({"api_version":1, "request_id":input["request_id"],
                    "outcome":"error", "error":{"message":format!("{error:#}"),"retryable":false}}),
            };
            println!("{response}");
            if response["outcome"] == "error" {
                std::process::exit(1);
            }
            Ok(())
        }
        Action::Serve { codex, workspace } => serve(cli.state_dir, codex, workspace),
    }
}

fn serve(state: PathBuf, codex: PathBuf, workspace: PathBuf) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&shutdown))?;
    }
    let workspace = workspace.canonicalize().context("workspace must exist")?;
    ensure!(workspace.is_dir(), "workspace must be a directory");
    let version = Command::new(&codex)
        .arg("--version")
        .output()
        .context("execute Codex; install the agapi/Codex pair")?;
    ensure!(
        version.status.success()
            && String::from_utf8_lossy(&version.stdout).trim()
                == format!("codex-cli {}", include_str!("../codex-version").trim()),
        "unsupported Codex version; agapi requires {}",
        include_str!("../codex-version").trim()
    );
    // Create private state before starting either socket or child process.
    drop(store::Store::open(&state)?);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state.join("serve.lock"))?;
    lock.try_lock()
        .context("agapi is already serving this state directory")?;
    let socket = state.join("codex.sock");
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    fs::write(
        state.join("workspace"),
        workspace.as_os_str().as_encoded_bytes(),
    )?;
    let mut child = Command::new(codex)
        .args(["app-server", "--listen"])
        .arg(format!("unix://{}", socket.display()))
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start Codex app server")?;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            child.kill()?;
            child.wait()?;
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("Codex app server exited: {status}");
        }
        if let Err(error) = api::reconcile(&state) {
            eprintln!("agapi reconciliation: {error:#}");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
