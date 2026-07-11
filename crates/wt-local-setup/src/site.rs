use crate::files::{require_root_file, sudo_install, sudo_move};
use crate::host;
use crate::image;
use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use nix::unistd::Uid;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use wt_libvirt::{SiteConfig, SITE_CONFIG_PATH};

pub(crate) fn install(runner: &impl Runner, config_path: &Path) -> Result<()> {
    require_site_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    require_installed_config_compatible(&config_bytes)?;
    prepare_host(runner, &config)?;
    image::ensure(runner, &config, &config_bytes)?;
    println!("Building and installing wt binaries...");
    build_and_install_binaries(runner, &config)?;
    println!("Installing site config at {SITE_CONFIG_PATH}...");
    install_config(runner, &config_bytes)?;
    println!("installed wt site from {}", config_path.display());
    Ok(())
}

pub(crate) fn image(runner: &impl Runner, config_path: &Path, rebuild: bool) -> Result<()> {
    require_site_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    prepare_host(runner, &config)?;
    if rebuild {
        image::rebuild(runner, &config, &config_bytes)?;
    } else {
        image::ensure(runner, &config, &config_bytes)?;
    }
    println!("image ready: {}", config.image.installed_path.display());
    Ok(())
}

fn prepare_host(runner: &impl Runner, config: &SiteConfig) -> Result<()> {
    host::preflight(runner)?;
    runner.run("sudo", &args(["-v"]), "authenticate sudo")?;
    host::prepare_state(runner, config)
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
    // The site file is a complete contract. Byte-level differences are drift, not migrations.
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
