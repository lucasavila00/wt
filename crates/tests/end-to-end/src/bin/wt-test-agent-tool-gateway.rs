use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use wt_agent_tool_gateway::{ActivityRecorder, Gateway, GatewayConfig, Provider};
use wt_libvirt_kvm::LibvirtProvider;
use wt_server::ServerConfig;

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-test-agent-tool-gateway: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let temp = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("expected temporary directory argument")?;
    let config =
        ServerConfig::load_runtime_from(&temp.join("server.toml")).map_err(anyhow::Error::msg)?;
    let provider = LibvirtProvider::new(config.machine_config()).map_err(anyhow::Error::msg)?;
    let gateway = Gateway::open(
        GatewayConfig {
            providers: vec![Provider::Local {
                host: "local.test".to_owned(),
                repositories: temp.clone(),
                api: None,
            }],
        },
        ActivityRecorder::open(&temp.join("worlds.db"))?,
    )?;
    serve_control(gateway.clone(), &temp.join("gateway-control.sock"))?;
    wt_agent_tool_gateway::start_vsock(gateway, config.agent_tools.vsock_port, move |cid| {
        provider
            .world_id_for_vsock_cid(cid)
            .map_err(anyhow::Error::msg)
    })?;
    loop {
        std::thread::park();
    }
}

fn serve_control(gateway: Gateway, path: &Path) -> Result<()> {
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("protect {}", path.display()))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let gateway = gateway.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = gateway.handle_control(stream) {
                            eprintln!("wt-test-agent-tool-gateway: control request: {error:#}");
                        }
                    });
                }
                Err(error) => eprintln!("wt-test-agent-tool-gateway: accept control: {error}"),
            }
        }
    });
    Ok(())
}
