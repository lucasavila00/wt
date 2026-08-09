use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use wt_agent_git::{Gateway, GatewayConfig, Provider, VsockListener, VSOCK_PORT};

#[derive(Debug, Parser)]
#[command(name = "wt-agent-git-gateway")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "/run/wt/agent-git-control.sock")]
        control_socket: PathBuf,
        #[arg(long, default_value = "/var/lib/wt/agent-git/state.json")]
        state_file: PathBuf,
        #[arg(long, value_parser = parse_local_provider)]
        local_provider: Vec<(String, PathBuf)>,
        #[arg(long)]
        transport_socket: Option<PathBuf>,
        #[arg(long, default_value_t = VSOCK_PORT)]
        vsock_port: u32,
        #[arg(long)]
        no_vsock: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-agent-git-gateway: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Command::Serve {
        control_socket,
        state_file,
        local_provider,
        transport_socket,
        vsock_port,
        no_vsock,
    } = Cli::parse().command;
    let providers = local_provider
        .into_iter()
        .map(|(host, repositories)| Provider::Local { host, repositories })
        .collect();
    let gateway = Gateway::open(GatewayConfig {
        state_file,
        providers,
    })?;
    let control = bind_unix(&control_socket, 0o600)?;
    let control_gateway = gateway.clone();
    std::thread::spawn(move || {
        for stream in control.incoming() {
            match stream {
                Ok(stream) => {
                    let gateway = control_gateway.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = gateway.handle_control(stream) {
                            eprintln!("wt-agent-git-gateway: control request: {error:#}");
                        }
                    });
                }
                Err(error) => eprintln!("wt-agent-git-gateway: accept control request: {error}"),
            }
        }
    });
    if let Some(path) = transport_socket {
        let listener = bind_unix(&path, 0o600)?;
        let transport_gateway = gateway.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let gateway = transport_gateway.clone();
                        std::thread::spawn(move || {
                            if let Err(error) = gateway.handle_transport(stream) {
                                eprintln!("wt-agent-git-gateway: transport request: {error:#}");
                            }
                        });
                    }
                    Err(error) => {
                        eprintln!("wt-agent-git-gateway: accept transport request: {error}")
                    }
                }
            }
        });
    }
    if no_vsock {
        loop {
            std::thread::park();
        }
    }
    let listener = VsockListener::bind(u32::MAX, vsock_port).context("bind gateway vsock")?;
    loop {
        let stream = listener.accept().context("accept gateway vsock")?;
        let gateway = gateway.clone();
        std::thread::spawn(move || {
            if let Err(error) = gateway.handle_transport(stream) {
                eprintln!("wt-agent-git-gateway: transport request: {error:#}");
            }
        });
    }
}

fn bind_unix(path: &Path, mode: u32) -> Result<UnixListener> {
    let parent = path.parent().context("Unix socket has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("remove stale {}", path.display()))
        }
    }
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("protect {}", path.display()))?;
    Ok(listener)
}

fn parse_local_provider(value: &str) -> Result<(String, PathBuf), String> {
    let (host, path) = value
        .split_once('=')
        .ok_or_else(|| "expected HOST=REPOSITORY_DIRECTORY".to_owned())?;
    if host.is_empty() || path.is_empty() {
        return Err("expected HOST=REPOSITORY_DIRECTORY".to_owned());
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("local provider repository directory must be absolute".to_owned());
    }
    if !path.is_dir() {
        return Err(format!(
            "local provider repository directory not found: {}",
            path.display()
        ));
    }
    Ok((host.to_owned(), path))
}
