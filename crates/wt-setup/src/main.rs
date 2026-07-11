use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use nix::unistd::{getgroups, Gid, Group, Uid, User};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};
use wt_libvirt::{SiteConfig, LIBVIRT_URI, SITE_CONFIG_PATH};

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const IMAGE_BUILD_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Debug, Parser)]
#[command(name = "wt-setup")]
struct Cli {
    #[command(subcommand)]
    command: SetupCommand,
}

#[derive(Debug, Subcommand)]
enum SetupCommand {
    /// Parse and validate a site config without changing the host.
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Install a complete local wt site from this source checkout.
    Install {
        #[arg(long)]
        config: PathBuf,
    },
    /// Build or verify the configured golden image.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    Build {
        #[arg(long)]
        config: PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wt-setup: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let runner = SystemRunner;
    match Cli::parse().command {
        SetupCommand::Validate { config } => {
            SiteConfig::load_from(&config).map_err(anyhow::Error::msg)?;
            println!("valid {}", config.display());
        }
        SetupCommand::Install { config } => install(&runner, &config)?,
        SetupCommand::Image {
            command: ImageCommand::Build { config },
        } => image_command(&runner, &config)?,
    }
    Ok(())
}

trait Runner {
    fn output(&self, program: &str, args: &[OsString]) -> Result<Output>;

    fn run(&self, program: &str, args: &[OsString], action: &str) -> Result<()> {
        let output = self.output(program, args)?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn text(&self, program: &str, args: &[OsString], action: &str) -> Result<String> {
        let output = self.output(program, args)?;
        if !output.status.success() {
            bail!(
                "{action}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        String::from_utf8(output.stdout).with_context(|| format!("decode output from {action}"))
    }
}

struct SystemRunner;

impl Runner for SystemRunner {
    fn output(&self, program: &str, args: &[OsString]) -> Result<Output> {
        Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("run {program}"))
    }

    fn run(&self, program: &str, args: &[OsString], action: &str) -> Result<()> {
        let status = Command::new(program)
            .args(args)
            .status()
            .with_context(|| format!("run {program}"))?;
        if !status.success() {
            bail!("{action}: command exited with {status}");
        }
        Ok(())
    }
}

fn install(runner: &impl Runner, config_path: &Path) -> Result<()> {
    require_site_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    require_installed_config_compatible(&config_bytes)?;
    preflight_host(runner)?;
    runner.run("sudo", &args(["-v"]), "authenticate sudo")?;
    prepare_host_state(runner, &config)?;
    ensure_image(runner, &config, &config_bytes)?;
    build_and_install_binaries(runner, &config)?;
    install_config(runner, &config_bytes)?;
    println!("installed wt site from {}", config_path.display());
    Ok(())
}

fn image_command(runner: &impl Runner, config_path: &Path) -> Result<()> {
    require_site_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    preflight_host(runner)?;
    runner.run("sudo", &args(["-v"]), "authenticate sudo")?;
    prepare_host_state(runner, &config)?;
    ensure_image(runner, &config, &config_bytes)?;
    println!("image ready: {}", config.image.installed_path.display());
    Ok(())
}

fn load_config(path: &Path) -> Result<(SiteConfig, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read config {}", path.display()))?;
    let config = SiteConfig::load_from(path).map_err(anyhow::Error::msg)?;
    Ok((config, bytes))
}

fn require_site_user() -> Result<()> {
    if Uid::effective().is_root() {
        bail!("run as the site user, not with sudo");
    }
    Ok(())
}

fn require_workspace() -> Result<()> {
    if !Path::new("Cargo.toml").is_file()
        || !Path::new("crates/wt-cli/Cargo.toml").is_file()
        || !Path::new("crates/wt-local/Cargo.toml").is_file()
    {
        bail!("run from the root of a wt source checkout");
    }
    Ok(())
}

fn preflight_host(runner: &impl Runner) -> Result<()> {
    require_host_platform()?;
    require_kvm()?;
    require_active_group("kvm")?;
    require_active_group("libvirt")?;
    runner.run(
        "virsh",
        &args(["-c", LIBVIRT_URI, "domcapabilities", "--virttype", "kvm"]),
        "verify libvirt KVM capability",
    )?;
    Ok(())
}

fn prepare_host_state(runner: &impl Runner, config: &SiteConfig) -> Result<()> {
    ensure_network(runner, &config.libvirt.network)?;
    ensure_directories(runner, config)?;
    Ok(())
}

fn require_host_platform() -> Result<()> {
    if std::env::consts::ARCH != "x86_64" {
        bail!("Ubuntu 24.04 amd64 is required");
    }
    let release = fs::read_to_string("/etc/os-release").context("read /etc/os-release")?;
    let id = os_release_value(&release, "ID");
    let version = os_release_value(&release, "VERSION_ID");
    if id.as_deref() != Some("ubuntu") || version.as_deref() != Some("24.04") {
        bail!("Ubuntu 24.04 amd64 is required");
    }
    Ok(())
}

fn os_release_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (actual_key, value) = line.split_once('=')?;
        (actual_key == key).then(|| value.trim_matches('"').to_owned())
    })
}

fn require_kvm() -> Result<()> {
    let metadata = fs::metadata("/dev/kvm").context("KVM is required: read /dev/kvm")?;
    if !metadata.file_type().is_char_device() {
        bail!("KVM is required: /dev/kvm is not a character device");
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .context("KVM is required: open /dev/kvm for read/write")?;
    Ok(())
}

fn require_active_group(name: &str) -> Result<()> {
    let group = Group::from_name(name)
        .with_context(|| format!("look up group {name}"))?
        .with_context(|| format!("required group does not exist: {name}"))?;
    let active = group.gid == Gid::effective()
        || getgroups()
            .context("read active process groups")?
            .contains(&group.gid);
    if !active {
        bail!("group {name} is not active; log out, log back in, and rerun");
    }
    Ok(())
}

fn ensure_network(runner: &impl Runner, network: &str) -> Result<()> {
    let info = runner.text(
        "virsh",
        &args(["-c", LIBVIRT_URI, "net-info", network]),
        "inspect configured libvirt network",
    )?;
    if !info.lines().any(|line| field_is(line, "Active", "yes")) {
        runner.run(
            "virsh",
            &args(["-c", LIBVIRT_URI, "net-start", network]),
            "start configured libvirt network",
        )?;
    }
    if !info.lines().any(|line| field_is(line, "Autostart", "yes")) {
        runner.run(
            "virsh",
            &args(["-c", LIBVIRT_URI, "net-autostart", network]),
            "enable configured libvirt network",
        )?;
    }
    Ok(())
}

fn field_is(line: &str, field: &str, value: &str) -> bool {
    line.split_once(':')
        .map(|(key, actual)| key.trim() == field && actual.trim() == value)
        .unwrap_or(false)
}

fn ensure_directories(runner: &impl Runner, config: &SiteConfig) -> Result<()> {
    let image_dir = config
        .image
        .installed_path
        .parent()
        .context("image.installed_path has no parent")?;
    ensure_directory(runner, image_dir, Uid::from_raw(0), Gid::from_raw(0), 0o755)?;
    ensure_directory(
        runner,
        &config.install.binary_dir,
        Uid::from_raw(0),
        Gid::from_raw(0),
        0o755,
    )?;

    let libvirt_gid = Group::from_name("libvirt")
        .context("look up libvirt group")?
        .context("required group does not exist: libvirt")?
        .gid;
    ensure_directory(
        runner,
        &config.libvirt.worlds_dir,
        Uid::effective(),
        libvirt_gid,
        0o2770,
    )
}

fn ensure_directory(
    runner: &impl Runner,
    path: &Path,
    uid: Uid,
    gid: Gid,
    mode: u32,
) -> Result<()> {
    if path.exists() {
        let metadata =
            fs::metadata(path).with_context(|| format!("inspect directory {}", path.display()))?;
        if !metadata.is_dir()
            || metadata.uid() != uid.as_raw()
            || metadata.gid() != gid.as_raw()
            || metadata.mode() & 0o7777 != mode
        {
            bail!(
                "directory drift at {}: expected uid={}, gid={}, mode={mode:04o}",
                path.display(),
                uid.as_raw(),
                gid.as_raw()
            );
        }
        return Ok(());
    }
    runner.run(
        "sudo",
        &[
            "install".into(),
            "-d".into(),
            "-o".into(),
            uid.as_raw().to_string().into(),
            "-g".into(),
            gid.as_raw().to_string().into(),
            "-m".into(),
            format!("{mode:04o}").into(),
            path.as_os_str().to_owned(),
        ],
        "create site directory",
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    version: u32,
    source_sha256: String,
    config_sha256: String,
    golden_sha256: String,
    packages: Vec<String>,
}

fn ensure_image(runner: &impl Runner, config: &SiteConfig, config_bytes: &[u8]) -> Result<()> {
    let manifest_path = manifest_path(&config.image.installed_path);
    match (config.image.installed_path.exists(), manifest_path.exists()) {
        (true, true) => {
            verify_installed_image(config, config_bytes, &manifest_path)?;
            return Ok(());
        }
        (false, false) => {}
        _ => bail!("image drift: image and manifest must either both exist or both be absent"),
    }

    let source = source_image(config, runner)?;
    build_image(runner, config, config_bytes, &source, &manifest_path)
}

fn source_image(config: &SiteConfig, runner: &impl Runner) -> Result<PathBuf> {
    let path = Path::new("imgs").join(SOURCE_IMAGE_NAME);
    fs::create_dir_all("imgs").context("create imgs directory")?;
    if path.exists() {
        require_sha(&path, &config.image.source_sha256, "source image")?;
        return Ok(path);
    }
    let temporary = path.with_extension("img.download");
    if temporary.exists() {
        bail!(
            "stale source image download exists: {}",
            temporary.display()
        );
    }
    runner.run(
        "curl",
        &[
            "-fL".into(),
            "--output".into(),
            temporary.as_os_str().to_owned(),
            config.image.source_url.clone().into(),
        ],
        "download pinned Ubuntu image",
    )?;
    if let Err(error) = require_sha(&temporary, &config.image.source_sha256, "downloaded image") {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).context("publish source image")?;
    Ok(path)
}

fn build_image(
    runner: &impl Runner,
    config: &SiteConfig,
    config_bytes: &[u8],
    source: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let build_dir = config.libvirt.worlds_dir.join(BUILD_NAME);
    let disk = build_dir.join("disk.qcow2");
    let seed = build_dir.join("seed.img");
    let user_data = build_dir.join("user-data");
    let meta_data = build_dir.join("meta-data");
    let prepared = build_dir.join("golden.qcow2");

    if build_dir.exists() || domain_exists(runner)? {
        bail!("stale image build state exists for {BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create image build directory")?;
    let result = build_image_inner(
        runner,
        config,
        config_bytes,
        source,
        manifest_path,
        &disk,
        &seed,
        &user_data,
        &meta_data,
        &prepared,
    );
    if result.is_err() {
        cleanup_failed_build(runner, &build_dir);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_image_inner(
    runner: &impl Runner,
    config: &SiteConfig,
    config_bytes: &[u8],
    source: &Path,
    manifest_path: &Path,
    disk: &Path,
    seed: &Path,
    user_data: &Path,
    meta_data: &Path,
    prepared: &Path,
) -> Result<()> {
    runner.run(
        "qemu-img",
        &[
            "convert".into(),
            "-p".into(),
            "-O".into(),
            "qcow2".into(),
            source.as_os_str().to_owned(),
            disk.as_os_str().to_owned(),
        ],
        "copy source image",
    )?;
    runner.run(
        "qemu-img",
        &[
            "resize".into(),
            disk.as_os_str().to_owned(),
            format!("{}G", config.guest.disk_gib).into(),
        ],
        "resize image build disk",
    )?;
    fs::write(user_data, cloud_config()).context("write image cloud-init user-data")?;
    fs::write(
        meta_data,
        format!("instance-id: {BUILD_NAME}\nlocal-hostname: {BUILD_NAME}\n"),
    )
    .context("write image cloud-init meta-data")?;
    runner.run(
        "cloud-localds",
        &[
            seed.as_os_str().to_owned(),
            user_data.as_os_str().to_owned(),
            meta_data.as_os_str().to_owned(),
        ],
        "create image build seed",
    )?;
    runner.run(
        "virt-install",
        &[
            "--connect".into(),
            LIBVIRT_URI.into(),
            "--name".into(),
            BUILD_NAME.into(),
            "--memory".into(),
            config.guest.memory_mib.to_string().into(),
            "--vcpus".into(),
            config.guest.vcpus.to_string().into(),
            "--virt-type".into(),
            "kvm".into(),
            "--os-variant".into(),
            "ubuntu24.04".into(),
            "--import".into(),
            "--boot".into(),
            "uefi".into(),
            "--disk".into(),
            format!("path={},format=qcow2,bus=virtio", disk.display()).into(),
            "--disk".into(),
            format!("path={},device=cdrom", seed.display()).into(),
            "--network".into(),
            format!("network={},model=virtio", config.libvirt.network).into(),
            "--graphics".into(),
            "none".into(),
            "--noautoconsole".into(),
            "--wait".into(),
            "0".into(),
        ],
        "start KVM image build guest",
    )?;
    wait_for_shutdown(runner)?;

    let marker = runner.text(
        "sudo",
        &[
            "virt-cat".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
            "/var/lib/wt-image-ready".into(),
        ],
        "verify image readiness marker",
    )?;
    if marker.trim() != "ready" {
        bail!("image build finished without the expected readiness marker");
    }
    let packages = runner
        .text(
            "sudo",
            &[
                "virt-cat".into(),
                "-a".into(),
                disk.as_os_str().to_owned(),
                "/var/lib/wt-image-packages".into(),
            ],
            "read installed guest package versions",
        )?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if packages.len() != 3 {
        bail!("image package manifest must contain exactly three packages");
    }

    undefine_build_domain(runner)?;
    runner.run(
        "sudo",
        &[
            "virt-sysprep".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
        ],
        "sysprep golden image",
    )?;
    let user = User::from_uid(Uid::effective())
        .context("look up site user")?
        .context("site user does not exist")?;
    runner.run(
        "sudo",
        &[
            "chown".into(),
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()).into(),
            disk.as_os_str().to_owned(),
        ],
        "restore image build disk ownership",
    )?;
    runner.run(
        "qemu-img",
        &[
            "convert".into(),
            "-p".into(),
            "-O".into(),
            "qcow2".into(),
            disk.as_os_str().to_owned(),
            prepared.as_os_str().to_owned(),
        ],
        "compact golden image",
    )?;
    runner.run(
        "qemu-img",
        &["check".into(), prepared.as_os_str().to_owned()],
        "check golden image",
    )?;

    let manifest = ImageManifest {
        version: 1,
        source_sha256: config.image.source_sha256.to_ascii_lowercase(),
        config_sha256: sha_bytes(config_bytes),
        golden_sha256: sha_file(prepared)?,
        packages,
    };
    publish_image(runner, config, prepared, manifest_path, &manifest)?;
    fs::remove_dir_all(config.libvirt.worlds_dir.join(BUILD_NAME))
        .context("remove image build directory")?;
    Ok(())
}

fn cloud_config() -> &'static str {
    r#"#cloud-config
package_update: true
packages:
  - docker.io
  - docker-compose-v2
  - qemu-guest-agent
runcmd:
  - systemctl enable --now docker.service qemu-guest-agent.service
  - docker info
  - docker compose version
  - dpkg-query -W -f='${Package}=${Version}\n' docker.io docker-compose-v2 qemu-guest-agent | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"#
}

fn wait_for_shutdown(runner: &impl Runner) -> Result<()> {
    let deadline = Instant::now() + IMAGE_BUILD_TIMEOUT;
    loop {
        let state = runner.text(
            "virsh",
            &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
            "read image build domain state",
        )?;
        if state.trim() == "shut off" {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for KVM image build guest");
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn domain_exists(runner: &impl Runner) -> Result<bool> {
    let output = runner.output("virsh", &args(["-c", LIBVIRT_URI, "dominfo", BUILD_NAME]))?;
    Ok(output.status.success())
}

fn undefine_build_domain(runner: &impl Runner) -> Result<()> {
    runner.run(
        "virsh",
        &args(["-c", LIBVIRT_URI, "undefine", BUILD_NAME, "--nvram"]),
        "undefine image build domain",
    )
}

fn cleanup_failed_build(runner: &impl Runner, build_dir: &Path) {
    if domain_exists(runner).unwrap_or(false) {
        let state = runner
            .text(
                "virsh",
                &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
                "read failed build domain state",
            )
            .unwrap_or_default();
        if state.trim() != "shut off" {
            let _ = runner.run(
                "virsh",
                &args(["-c", LIBVIRT_URI, "destroy", BUILD_NAME]),
                "destroy failed build domain",
            );
        }
        let _ = undefine_build_domain(runner);
    }
    let _ = fs::remove_dir_all(build_dir);
}

fn publish_image(
    runner: &impl Runner,
    config: &SiteConfig,
    prepared: &Path,
    manifest_path: &Path,
    manifest: &ImageManifest,
) -> Result<()> {
    let image_temporary = sibling_temporary(&config.image.installed_path)?;
    let manifest_temporary = sibling_temporary(manifest_path)?;
    if image_temporary.exists() || manifest_temporary.exists() {
        bail!("stale temporary installed image state exists");
    }
    let local_manifest = prepared.with_extension("manifest.json");
    fs::write(&local_manifest, serde_json::to_vec_pretty(manifest)?)
        .context("write image manifest")?;
    sudo_install(runner, prepared, &image_temporary, 0o644)?;
    sudo_install(runner, &local_manifest, &manifest_temporary, 0o644)?;
    sudo_move(runner, &image_temporary, &config.image.installed_path)?;
    sudo_move(runner, &manifest_temporary, manifest_path)?;
    Ok(())
}

fn verify_installed_image(
    config: &SiteConfig,
    config_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read image manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse image manifest {}", manifest_path.display()))?;
    if manifest.version != 1
        || manifest.source_sha256 != config.image.source_sha256.to_ascii_lowercase()
        || manifest.config_sha256 != sha_bytes(config_bytes)
        || manifest.packages.len() != 3
    {
        bail!("installed image provenance differs from requested config");
    }
    require_sha(
        &config.image.installed_path,
        &manifest.golden_sha256,
        "installed golden image",
    )
}

fn build_and_install_binaries(runner: &impl Runner, config: &SiteConfig) -> Result<()> {
    runner.run(
        "cargo",
        &args(["build", "--release", "-p", "wt-cli", "-p", "wt-local"]),
        "build wt binaries",
    )?;
    for name in ["wt", "wt-local"] {
        let source = Path::new("target/release").join(name);
        let destination = config.install.binary_dir.join(name);
        let temporary = config.install.binary_dir.join(format!(".{name}.wt-new"));
        if temporary.exists() {
            bail!("stale binary install file exists: {}", temporary.display());
        }
        sudo_install(runner, &source, &temporary, 0o755)?;
        sudo_move(runner, &temporary, &destination)?;
    }
    Ok(())
}

fn require_installed_config_compatible(config_bytes: &[u8]) -> Result<()> {
    let path = Path::new(SITE_CONFIG_PATH);
    if !path.exists() {
        return Ok(());
    }
    let installed = fs::read(path).with_context(|| format!("read {SITE_CONFIG_PATH}"))?;
    if installed != config_bytes {
        bail!("installed config differs from requested config: {SITE_CONFIG_PATH}");
    }
    require_root_file(path, 0o644)
}

fn install_config(runner: &impl Runner, config_bytes: &[u8]) -> Result<()> {
    if Path::new(SITE_CONFIG_PATH).exists() {
        return require_installed_config_compatible(config_bytes);
    }
    let directory = Path::new(SITE_CONFIG_PATH)
        .parent()
        .context("site config has no parent directory")?;
    if directory.exists() {
        let metadata = fs::metadata(directory).context("inspect /etc/wt")?;
        if metadata.uid() != 0 || metadata.gid() != 0 || metadata.mode() & 0o7777 != 0o755 {
            bail!("directory drift at /etc/wt: expected uid=0, gid=0, mode=0755");
        }
    } else {
        runner.run(
            "sudo",
            &args([
                "install", "-d", "-o", "root", "-g", "root", "-m", "0755", "/etc/wt",
            ]),
            "create /etc/wt",
        )?;
    }
    let local = Path::new("target").join("wt-local.toml.install");
    fs::write(&local, config_bytes).context("stage site config")?;
    let temporary = Path::new("/etc/wt/.local.toml.wt-new");
    if temporary.exists() {
        bail!("stale config install file exists: {}", temporary.display());
    }
    sudo_install(runner, &local, temporary, 0o644)?;
    sudo_move(runner, temporary, Path::new(SITE_CONFIG_PATH))?;
    let _ = fs::remove_file(local);
    Ok(())
}

fn require_root_file(path: &Path, mode: u32) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.gid() != 0
        || metadata.mode() & 0o7777 != mode
    {
        bail!(
            "file drift at {}: expected uid=0, gid=0, mode={mode:04o}",
            path.display()
        );
    }
    Ok(())
}

fn sudo_install(runner: &impl Runner, source: &Path, destination: &Path, mode: u32) -> Result<()> {
    runner.run(
        "sudo",
        &[
            "install".into(),
            "-o".into(),
            "root".into(),
            "-g".into(),
            "root".into(),
            "-m".into(),
            format!("{mode:04o}").into(),
            source.as_os_str().to_owned(),
            destination.as_os_str().to_owned(),
        ],
        "install file",
    )
}

fn sudo_move(runner: &impl Runner, source: &Path, destination: &Path) -> Result<()> {
    runner.run(
        "sudo",
        &[
            "mv".into(),
            "--".into(),
            source.as_os_str().to_owned(),
            destination.as_os_str().to_owned(),
        ],
        "publish installed file",
    )
}

fn manifest_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.manifest.json", image.display()))
}

fn sibling_temporary(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .context("installed path has no file name")?
        .to_string_lossy();
    Ok(path.with_file_name(format!(".{name}.wt-new")))
}

fn require_sha(path: &Path, expected: &str, description: &str) -> Result<()> {
    let actual = sha_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("{description} SHA-256 mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

fn sha_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).with_context(|| format!("hash {}", path.display()))?;
    Ok(format!("{:x}", digest.finalize()))
}

fn sha_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn args<const N: usize>(values: [&str; N]) -> Vec<OsString> {
    values.into_iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::os::unix::process::ExitStatusExt;

    struct FakeRunner {
        outputs: RefCell<VecDeque<Output>>,
        calls: RefCell<Vec<(String, Vec<OsString>)>>,
    }

    impl FakeRunner {
        fn new(outputs: impl IntoIterator<Item = (&'static str, bool)>) -> Self {
            Self {
                outputs: RefCell::new(
                    outputs
                        .into_iter()
                        .map(|(stdout, success)| Output {
                            status: std::process::ExitStatus::from_raw(if success { 0 } else { 1 }),
                            stdout: stdout.as_bytes().to_vec(),
                            stderr: Vec::new(),
                        })
                        .collect(),
                ),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Runner for FakeRunner {
        fn output(&self, program: &str, args: &[OsString]) -> Result<Output> {
            self.calls
                .borrow_mut()
                .push((program.to_owned(), args.to_vec()));
            self.outputs
                .borrow_mut()
                .pop_front()
                .context("unexpected fake command")
        }
    }

    #[test]
    fn virsh_fields_are_parsed_exactly() {
        assert!(field_is("Active:         yes", "Active", "yes"));
        assert!(!field_is("Active:         no", "Active", "yes"));
        assert!(!field_is("Inactive:       yes", "Active", "yes"));
    }

    #[test]
    fn os_release_values_are_parsed_exactly() {
        let release = "ID=ubuntu\nVERSION_ID=\"24.04\"\n";
        assert_eq!(os_release_value(release, "ID").as_deref(), Some("ubuntu"));
        assert_eq!(
            os_release_value(release, "VERSION_ID").as_deref(),
            Some("24.04")
        );
    }

    #[test]
    fn matching_network_is_not_mutated() {
        let runner = FakeRunner::new([("Active: yes\nAutostart: yes\n", true)]);
        ensure_network(&runner, "default").unwrap();
        assert_eq!(runner.calls.borrow().len(), 1);
    }

    #[test]
    fn inactive_network_is_started_and_enabled() {
        let runner = FakeRunner::new([
            ("Active: no\nAutostart: no\n", true),
            ("", true),
            ("", true),
        ]);
        ensure_network(&runner, "site").unwrap();
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert!(calls[1].1.iter().any(|argument| argument == "net-start"));
        assert!(calls[2]
            .1
            .iter()
            .any(|argument| argument == "net-autostart"));
    }

    #[test]
    fn manifest_path_is_next_to_image() {
        assert_eq!(
            manifest_path(Path::new("/var/lib/wt/golden.qcow2")),
            Path::new("/var/lib/wt/golden.qcow2.manifest.json")
        );
    }

    #[test]
    fn sha_validation_detects_drift() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("image");
        fs::write(&file, b"expected").unwrap();
        let expected = sha_bytes(b"expected");
        require_sha(&file, &expected, "test image").unwrap();
        fs::write(&file, b"different").unwrap();
        assert!(require_sha(&file, &expected, "test image").is_err());
    }
}
