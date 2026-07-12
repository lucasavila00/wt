use crate::files::sudo_install;
use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use wt_libvirt::ServerConfig;

pub(crate) const CONTAINER_NAME: &str = "wt-registry-cache";
pub(crate) const PROXY_IMAGE: &str = "rpardini/docker-registry-proxy@sha256:b70b2ef2371171a630e3fcbf2217e04057c1dbe114fa46d332ebde67349869e9";
const CA_INSTALL_PATH: &str = "/usr/local/share/ca-certificates/wt-registry-cache.crt";
const DOCKER_DROP_IN_DIR: &str = "/etc/systemd/system/docker.service.d";
const DOCKER_DROP_IN_PATH: &str = "/etc/systemd/system/docker.service.d/wt-registry-cache.conf";

#[derive(Serialize)]
struct CacheManifest<'a> {
    version: u32,
    proxy_image: &'a str,
    bridge_address: &'a str,
    port: u16,
    max_size_gib: u64,
    registries: &'a [String],
    images: Vec<CachedImage>,
}

#[derive(Serialize)]
struct CachedImage {
    image: String,
    repo_digests: Vec<String>,
}

pub(crate) fn ensure(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let bridge = bridge_address(runner, &config.libvirt.network)?;
    ensure_state_directories(runner, &config.registry_cache.state_dir)?;
    ensure_container(runner, config, &bridge)?;
    let ca = wait_for_ca(&config.registry_cache.state_dir)?;
    wait_for_proxy(runner, &bridge, config.registry_cache.port)?;
    runner.run(
        "sudo",
        &["chmod".into(), "0644".into(), ca.as_os_str().to_owned()],
        "make registry cache CA readable",
    )?;
    configure_host_docker(runner, &ca, &bridge, config.registry_cache.port)?;
    wait_for_proxy(runner, &bridge, config.registry_cache.port)?;
    let images = preload(runner, config, &bridge)?;
    publish_manifest(runner, config, &bridge, images)?;
    Ok(())
}

fn bridge_address(runner: &impl Runner, network: &str) -> Result<String> {
    let xml = runner.text(
        "virsh",
        &args(["-c", wt_libvirt::LIBVIRT_URI, "net-dumpxml", network]),
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
            "sudo",
            &[
                "install".into(),
                "-d".into(),
                "-o".into(),
                "root".into(),
                "-g".into(),
                "root".into(),
                "-m".into(),
                "0755".into(),
                path.as_os_str().to_owned(),
            ],
            "create registry cache state directory",
        )?;
    }
    Ok(())
}

fn ensure_container(runner: &impl Runner, config: &ServerConfig, bridge: &str) -> Result<()> {
    let existing = runner.output("docker", &args(["container", "inspect", CONTAINER_NAME]))?;
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
            "docker",
            &args(["container", "start", CONTAINER_NAME]),
            "start registry cache container",
        )?;
        return Ok(());
    }

    runner.run(
        "docker",
        &["pull".into(), PROXY_IMAGE.into()],
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
    let mut command: Vec<OsString> = [
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
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if !registries.is_empty() {
        command.extend(["--env".into(), format!("REGISTRIES={registries}").into()]);
    }
    command.push(PROXY_IMAGE.into());
    runner.run("docker", &command, "create registry cache container")
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

fn configure_host_docker(runner: &impl Runner, ca: &Path, bridge: &str, port: u16) -> Result<()> {
    sudo_install(runner, ca, Path::new(CA_INSTALL_PATH), 0o644)?;
    runner.run(
        "sudo",
        &args(["install", "-d", "-m", "0755", DOCKER_DROP_IN_DIR]),
        "create Docker systemd drop-in directory",
    )?;
    let staged = Path::new("target/wt-registry-cache-docker.conf");
    fs::write(
        staged,
        format!(
            "[Service]\nEnvironment=\"HTTP_PROXY=http://{bridge}:{port}/\"\nEnvironment=\"HTTPS_PROXY=http://{bridge}:{port}/\"\nEnvironment=\"NO_PROXY=localhost,127.0.0.1,{bridge}\"\n"
        ),
    )
    .context("stage Docker registry cache proxy configuration")?;
    sudo_install(runner, staged, Path::new(DOCKER_DROP_IN_PATH), 0o644)?;
    let _ = fs::remove_file(staged);
    runner.run(
        "sudo",
        &args(["update-ca-certificates"]),
        "trust registry cache CA",
    )?;
    runner.run(
        "sudo",
        &args(["systemctl", "daemon-reload"]),
        "reload systemd",
    )?;
    runner.run(
        "sudo",
        &args(["systemctl", "restart", "docker.service"]),
        "restart Docker with registry cache proxy",
    )
}

fn wait_for_proxy(runner: &impl Runner, bridge: &str, port: u16) -> Result<()> {
    let url = format!("http://{bridge}:{port}/ca.crt");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if runner
            .output("curl", &["-fsS".into(), url.clone().into()])?
            .status
            .success()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("registry cache proxy is not reachable at {url}")
}

fn preload(runner: &impl Runner, config: &ServerConfig, bridge: &str) -> Result<Vec<CachedImage>> {
    let mut cached = Vec::new();
    let proxy = format!("http://{bridge}:{}", config.registry_cache.port);
    for (index, image) in config.registry_cache.preload_images.iter().enumerate() {
        println!("Preloading registry cache image {image}...");
        let image_source = format!("docker://{}", canonical_image(image));
        let first = Path::new("target").join(format!("wt-cache-preload-{index}-first"));
        let second = Path::new("target").join(format!("wt-cache-preload-{index}-second"));
        if first.exists() || second.exists() {
            bail!("stale registry cache preload state for {image}");
        }
        skopeo_copy(
            runner,
            &proxy,
            &image_source,
            &first,
            "preload registry cache image",
        )?;
        let digests = runner.text(
            "env",
            &[
                format!("HTTP_PROXY={proxy}").into(),
                format!("HTTPS_PROXY={proxy}").into(),
                "skopeo".into(),
                "inspect".into(),
                "--format".into(),
                "{{.Digest}}".into(),
                image_source.clone().into(),
            ],
            "inspect preloaded registry cache image",
        )?;
        let digest = digests.trim();
        if !digest.starts_with("sha256:") {
            bail!("preloaded image has no repository digest: {image}");
        }
        fs::remove_dir_all(&first).context("remove first registry cache preload copy")?;
        let since = unix_timestamp();
        skopeo_copy(
            runner,
            &proxy,
            &image_source,
            &second,
            "verify preloaded registry cache image",
        )?;
        if cache_hits_since(runner, since)? == 0 {
            bail!("preloaded image was not served from the registry cache: {image}");
        }
        fs::remove_dir_all(&second).context("remove second registry cache preload copy")?;
        cached.push(CachedImage {
            image: image.clone(),
            repo_digests: vec![format!("{}@{digest}", canonical_image(image))],
        });
    }
    Ok(cached)
}

fn skopeo_copy(
    runner: &impl Runner,
    proxy: &str,
    source: &str,
    destination: &Path,
    action: &str,
) -> Result<()> {
    runner.run(
        "env",
        &[
            format!("HTTP_PROXY={proxy}").into(),
            format!("HTTPS_PROXY={proxy}").into(),
            "skopeo".into(),
            "copy".into(),
            "--override-os".into(),
            "linux".into(),
            "--override-arch".into(),
            "amd64".into(),
            source.into(),
            format!("dir:{}", destination.display()).into(),
        ],
        action,
    )
}

fn canonical_image(image: &str) -> String {
    let first = image.split('/').next().unwrap_or_default();
    if image.contains('/') && (first.contains('.') || first.contains(':') || first == "localhost") {
        return image.to_owned();
    }
    if image.contains('/') {
        format!("docker.io/{image}")
    } else {
        format!("docker.io/library/{image}")
    }
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_hits_since(runner: &impl Runner, since: u64) -> Result<usize> {
    let output = runner.output(
        "docker",
        &[
            "logs".into(),
            "--since".into(),
            since.to_string().into(),
            CONTAINER_NAME.into(),
        ],
    )?;
    if !output.status.success() {
        bail!(
            "read registry cache logs: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["upstream_cache_status"].as_str() == Some("HIT"))
        .count())
}

fn publish_manifest(
    runner: &impl Runner,
    config: &ServerConfig,
    bridge: &str,
    images: Vec<CachedImage>,
) -> Result<()> {
    let manifest = CacheManifest {
        version: 1,
        proxy_image: PROXY_IMAGE,
        bridge_address: bridge,
        port: config.registry_cache.port,
        max_size_gib: config.registry_cache.max_size_gib,
        registries: &config.registry_cache.registries,
        images,
    };
    let staged = Path::new("target/wt-registry-cache-manifest.json");
    fs::write(staged, serde_json::to_vec_pretty(&manifest)?)
        .context("stage registry cache manifest")?;
    sudo_install(
        runner,
        staged,
        &config.registry_cache.state_dir.join("manifest.json"),
        0o644,
    )?;
    let _ = fs::remove_file(staged);
    Ok(())
}
