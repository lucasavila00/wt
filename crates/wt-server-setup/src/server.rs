use crate::files::{require_root_file, sudo_install, sudo_move};
use crate::host;
use crate::image;
use crate::registry_cache;
use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use nix::unistd::Uid;
use ssh_key::PrivateKey;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use wt_libvirt::{GitConfig, ServerConfig, SERVER_CONFIG_PATH};

pub(crate) fn install(runner: &impl Runner, config_path: &Path) -> Result<()> {
    require_server_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    require_installed_config_compatible(&config_bytes)?;
    prepare_host(runner, &config)?;
    registry_cache::ensure(runner, &config)?;
    image::ensure(runner, &config, &config_bytes)?;
    println!("Building and installing wt binaries...");
    build_and_install_binaries(runner, &config)?;
    println!("Installing server config at {SERVER_CONFIG_PATH}...");
    install_config(runner, &config_bytes)?;
    println!("installed wt server from {}", config_path.display());
    Ok(())
}

pub(crate) fn validate(config_path: &Path) -> Result<()> {
    load_config(config_path).map(|_| ())
}

pub(crate) fn image(runner: &impl Runner, config_path: &Path, rebuild: bool) -> Result<()> {
    require_server_user()?;
    let (config, config_bytes) = load_config(config_path)?;
    require_workspace()?;
    prepare_host(runner, &config)?;
    registry_cache::ensure(runner, &config)?;
    if rebuild {
        image::rebuild(runner, &config, &config_bytes)?;
    } else {
        image::ensure(runner, &config, &config_bytes)?;
    }
    println!("image ready: {}", config.image.installed_path.display());
    Ok(())
}

fn prepare_host(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    host::preflight(runner)?;
    runner.run("sudo", &args(["-v"]), "authenticate sudo")?;
    host::prepare_state(runner, config)
}

fn load_config(path: &Path) -> Result<(ServerConfig, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read config {}", path.display()))?;
    let config = ServerConfig::load_from(path).map_err(anyhow::Error::msg)?;
    let git = config.resolved_git_config().map_err(anyhow::Error::msg)?;
    validate_git_credentials(&git)?;
    Ok((config, bytes))
}

fn validate_git_credentials(config: &GitConfig) -> Result<()> {
    let identity = &config.identity_file;
    let metadata = fs::metadata(identity)
        .with_context(|| format!("inspect git.identity_file {}", identity.display()))?;
    if !metadata.is_file()
        || metadata.uid() != Uid::effective().as_raw()
        || metadata.mode() & 0o7777 != 0o600
    {
        bail!(
            "git.identity_file {} must be a regular file owned by the server user with mode 0600",
            identity.display()
        );
    }
    let encoded = fs::read_to_string(identity)
        .with_context(|| format!("read git.identity_file {}", identity.display()))?;
    let private_key = PrivateKey::from_openssh(&encoded)
        .with_context(|| format!("parse git.identity_file {}", identity.display()))?;
    if !private_key.is_encrypted() {
        bail!(
            "git.identity_file {} must be an encrypted OpenSSH private key",
            identity.display()
        );
    }

    let known_hosts = &config.known_hosts_file;
    let metadata = fs::metadata(known_hosts)
        .with_context(|| format!("inspect git.known_hosts_file {}", known_hosts.display()))?;
    if !metadata.is_file() {
        bail!(
            "git.known_hosts_file {} must be a regular file",
            known_hosts.display()
        );
    }
    let contents = fs::read_to_string(known_hosts)
        .with_context(|| format!("read git.known_hosts_file {}", known_hosts.display()))?;
    let has_entries = contents
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#'));
    let output = Command::new("ssh-keygen")
        .args(["-l", "-f"])
        .arg(known_hosts)
        .output()
        .with_context(|| format!("validate git.known_hosts_file {}", known_hosts.display()))?;
    if !has_entries || !output.status.success() {
        bail!(
            "git.known_hosts_file {} must contain valid known-hosts entries",
            known_hosts.display()
        );
    }
    Ok(())
}

fn require_server_user() -> Result<()> {
    if Uid::effective().is_root() {
        bail!("run as the server user, not with sudo");
    }
    Ok(())
}

fn require_workspace() -> Result<()> {
    if !Path::new("Cargo.toml").is_file()
        || !Path::new("crates/wt-cli/Cargo.toml").is_file()
        || !Path::new("crates/wt-guest/Cargo.toml").is_file()
        || !Path::new("crates/wt-server/Cargo.toml").is_file()
    {
        bail!("run from the root of a wt source checkout");
    }
    Ok(())
}

fn build_and_install_binaries(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    runner.run(
        "cargo",
        &args([
            "build",
            "--release",
            "-p",
            "wt-cli",
            "-p",
            "wt-guest",
            "-p",
            "wt-server",
        ]),
        "build wt binaries",
    )?;
    for name in ["wt", "wt-app-shell", "wt-server"] {
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
    let path = Path::new(SERVER_CONFIG_PATH);
    if !path.exists() {
        return Ok(());
    }
    // The server file is a complete contract. Byte-level differences are drift.
    let installed = fs::read(path).with_context(|| format!("read {SERVER_CONFIG_PATH}"))?;
    if installed != config_bytes {
        bail!("installed config differs from requested config: {SERVER_CONFIG_PATH}");
    }
    require_root_file(path, 0o644)
}

fn install_config(runner: &impl Runner, config_bytes: &[u8]) -> Result<()> {
    if Path::new(SERVER_CONFIG_PATH).exists() {
        return require_installed_config_compatible(config_bytes);
    }
    let directory = Path::new(SERVER_CONFIG_PATH)
        .parent()
        .context("server config has no parent directory")?;
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
    let local = Path::new("target").join("wt-server.toml.install");
    fs::write(&local, config_bytes).context("stage server config")?;
    let temporary = Path::new("/etc/wt/.server.toml.wt-new");
    if temporary.exists() {
        bail!("stale config install file exists: {}", temporary.display());
    }
    sudo_install(runner, &local, temporary, 0o644)?;
    sudo_move(runner, temporary, Path::new(SERVER_CONFIG_PATH))?;
    let _ = fs::remove_file(local);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn validates_server_owned_git_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let identity = temp.path().join("identity");
        let output = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "secret", "-f"])
            .arg(&identity)
            .output()
            .unwrap();
        assert!(output.status.success());
        fs::set_permissions(&identity, fs::Permissions::from_mode(0o600)).unwrap();
        let public = fs::read_to_string(identity.with_extension("pub")).unwrap();
        let mut fields = public.split_whitespace();
        let known_hosts = temp.path().join("known_hosts");
        fs::write(
            &known_hosts,
            format!(
                "example.test {} {}\n",
                fields.next().unwrap(),
                fields.next().unwrap()
            ),
        )
        .unwrap();
        let config = GitConfig {
            identity_file: identity.clone(),
            known_hosts_file: known_hosts,
        };
        validate_git_credentials(&config).unwrap();

        fs::set_permissions(identity, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(validate_git_credentials(&config).is_err());
    }
}
