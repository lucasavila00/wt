use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use wt_command::cmd;
use wt_server::ServerConfig;

pub(crate) const CONTAINER_NAME: &str = "wt-registry-cache";
pub(crate) const PROXY_IMAGE: &str = "rpardini/docker-registry-proxy@sha256:b70b2ef2371171a630e3fcbf2217e04057c1dbe114fa46d332ebde67349869e9";

pub(crate) fn ensure(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let bridge = bridge_address(runner, &config.libvirt.network)?;
    ensure_state_directories(runner, &config.registry_cache.state_dir)?;
    ensure_container(runner, config, &bridge)?;
    let ca = wait_for_ca(&config.registry_cache.state_dir)?;
    wait_for_proxy(runner, &bridge, config.registry_cache.port)?;
    runner.run(
        cmd!("sudo", "chmod", "0644", &ca),
        "make registry cache CA readable",
    )?;
    Ok(())
}

fn bridge_address(runner: &impl Runner, network: &str) -> Result<String> {
    let xml = runner.text(
        cmd!(
            "virsh",
            "-c",
            wt_libvirt::LIBVIRT_URI,
            "net-dumpxml",
            network,
        ),
        "read libvirt network XML",
    )?;
    for quote in ['\'', '"'] {
        let needle = format!("address={quote}");
        for rest in xml.split(&needle).skip(1) {
            if let Some(address) = rest.split(quote).next() {
                if address.parse::<std::net::Ipv4Addr>().is_ok() {
                    return Ok(address.to_owned());
                }
            }
        }
    }
    bail!("configured libvirt network has no IPv4 bridge address")
}

fn ensure_state_directories(runner: &impl Runner, state: &Path) -> Result<()> {
    for name in ["cache", "ca"] {
        let path = state.join(name);
        if path.exists() {
            if !path.is_dir() {
                bail!("registry cache state drift at {}", path.display());
            }
            continue;
        }
        runner.run(
            cmd!("sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "0755", &path,),
            "create registry cache state directory",
        )?;
    }
    Ok(())
}

fn ensure_container(runner: &impl Runner, config: &ServerConfig, bridge: &str) -> Result<()> {
    let existing = runner.output(cmd!("docker", "container", "inspect", CONTAINER_NAME))?;
    if existing.status.success() {
        let inspect: Vec<serde_json::Value> = serde_json::from_slice(&existing.stdout)
            .context("parse registry cache container inspection")?;
        let image = inspect
            .first()
            .and_then(|value| value.pointer("/Config/Image"))
            .and_then(serde_json::Value::as_str);
        let value = inspect
            .first()
            .context("empty registry cache container inspection")?;
        let expected_port = config.registry_cache.port.to_string();
        let expected_cache = format!(
            "{}:/docker_mirror_cache",
            config.registry_cache.state_dir.join("cache").display()
        );
        let expected_ca = format!(
            "{}:/ca",
            config.registry_cache.state_dir.join("ca").display()
        );
        let env = value
            .pointer("/Config/Env")
            .and_then(serde_json::Value::as_array)
            .context("registry cache container has no environment")?;
        let binds = value
            .pointer("/HostConfig/Binds")
            .and_then(serde_json::Value::as_array)
            .context("registry cache container has no bind mounts")?;
        let host_port = value
            .pointer("/HostConfig/PortBindings/3128~1tcp/0/HostPort")
            .and_then(serde_json::Value::as_str);
        let host_ip = value
            .pointer("/HostConfig/PortBindings/3128~1tcp/0/HostIp")
            .and_then(serde_json::Value::as_str);
        let has = |values: &[serde_json::Value], expected: &str| {
            values.iter().any(|value| value.as_str() == Some(expected))
        };
        if image != Some(PROXY_IMAGE)
            || host_port != Some(expected_port.as_str())
            || host_ip != Some(bridge)
            || !has(binds, &expected_cache)
            || !has(binds, &expected_ca)
            || !has(env, "ALLOW_PUSH=true")
            || !has(env, "ENABLE_MANIFEST_CACHE=false")
            || !has(
                env,
                &format!("CACHE_MAX_SIZE={}g", config.registry_cache.max_size_gib),
            )
        {
            bail!("registry cache container configuration drift");
        }
        runner.run(
            cmd!("docker", "container", "start", CONTAINER_NAME),
            "start registry cache container",
        )?;
        return Ok(());
    }

    runner.run(
        cmd!("docker", "pull", PROXY_IMAGE),
        "pull pinned registry cache image",
    )?;
    let registries = config
        .registry_cache
        .registries
        .iter()
        .filter(|registry| registry.as_str() != "docker.io")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let publish = format!("{bridge}:{}:3128", config.registry_cache.port);
    let cache = format!(
        "{}:/docker_mirror_cache",
        config.registry_cache.state_dir.join("cache").display()
    );
    let ca = format!(
        "{}:/ca",
        config.registry_cache.state_dir.join("ca").display()
    );
    let size = format!("CACHE_MAX_SIZE={}g", config.registry_cache.max_size_gib);
    let mut command = cmd!(
        "docker",
        "run",
        "--detach",
        "--name",
        CONTAINER_NAME,
        "--restart",
        "always",
        "--publish",
        &publish,
        "--env",
        "ALLOW_PUSH=true",
        "--env",
        "ENABLE_MANIFEST_CACHE=false",
        "--env",
        &size,
        "--volume",
        &cache,
        "--volume",
        &ca,
    );
    if !registries.is_empty() {
        command.arg("--env").arg(format!("REGISTRIES={registries}"));
    }
    command.arg(PROXY_IMAGE);
    runner.run(command, "create registry cache container")
}

fn wait_for_ca(state: &Path) -> Result<PathBuf> {
    let ca = state.join("ca/ca.crt");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if ca.is_file() {
            return Ok(ca);
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("registry cache did not publish its CA certificate")
}

fn wait_for_proxy(runner: &impl Runner, bridge: &str, port: u16) -> Result<()> {
    let url = format!("http://{bridge}:{port}/ca.crt");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if runner.output(cmd!("curl", "-fsS", &url))?.status.success() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("registry cache proxy is not reachable at {url}")
}
