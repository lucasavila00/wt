use crate::files::{require_root_file, sudo_install, sudo_move};
use crate::host;
use crate::image;
use crate::install_input::{serialize_server_config, InstallInput};
use crate::registry_cache;
use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use ssh_key::PrivateKey;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use wt_command::cmd;
use wt_libvirt::{GitConfig, ServerConfig, SERVER_CONFIG_PATH};

const SERVER_SERVICE_PATH: &str = "/etc/systemd/system/wt-server.service";

pub(crate) fn install(runner: &impl Runner, input_path: &Path) -> Result<()> {
    require_server_user()?;
    let (input, server, server_bytes) = load_install_input(input_path)?;
    require_workspace()?;
    require_installed_config_compatible(input_path, &server)?;
    prepare_host(runner, &server)?;
    registry_cache::ensure(runner, &server)?;
    image::ensure(runner, &input, &server, &server_bytes)?;
    println!("Building and installing wt binaries...");
    build_and_install_binaries(runner, &server)?;
    println!("Installing server config at {SERVER_CONFIG_PATH}...");
    install_server_config(runner, input_path, &server, &server_bytes)?;
    println!("Installing and starting wt-server.service...");
    install_server_service(runner, &server)?;
    println!(
        "installed wt server from install input {}",
        input_path.display()
    );
    Ok(())
}

pub(crate) fn validate(input_path: &Path) -> Result<()> {
    load_install_input(input_path).map(|_| ())
}

pub(crate) fn image(runner: &impl Runner, input_path: &Path, rebuild: bool) -> Result<()> {
    require_server_user()?;
    let (input, server, server_bytes) = load_install_input(input_path)?;
    require_workspace()?;
    prepare_host(runner, &server)?;
    registry_cache::ensure(runner, &server)?;
    if rebuild {
        image::rebuild(runner, &input, &server, &server_bytes)?;
    } else {
        image::ensure(runner, &input, &server, &server_bytes)?;
    }
    println!("image ready: {}", server.image.installed_path.display());
    Ok(())
}

fn prepare_host(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    host::preflight(runner)?;
    runner.run(cmd!("sudo", "-v"), "authenticate sudo")?;
    host::prepare_state(runner, config)
}

fn load_install_input(path: &Path) -> Result<(InstallInput, ServerConfig, Vec<u8>)> {
    let input = InstallInput::load_from(path).map_err(anyhow::Error::msg)?;
    let server = input.materialize();
    let server_bytes = serialize_server_config(&server).map_err(anyhow::Error::msg)?;
    let git = server.resolved_git_config().map_err(anyhow::Error::msg)?;
    validate_git_credentials(&git)?;
    Ok((input, server, server_bytes))
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
    let output = cmd!("ssh-keygen", "-l", "-f", known_hosts)
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
        cmd!(
            "cargo",
            "build",
            "--release",
            "-p",
            "wt-cli",
            "-p",
            "wt-guest",
            "-p",
            "wt-server",
        ),
        "build wt binaries",
    )?;
    for name in [
        "wt",
        "wt-app-pane",
        "wt-app-info",
        "wt-app-proxy",
        "wt-app-shell",
        "wt-server",
    ] {
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

fn require_installed_config_compatible(input_path: &Path, requested: &ServerConfig) -> Result<()> {
    let path = Path::new(SERVER_CONFIG_PATH);
    if !path.exists() {
        return Ok(());
    }
    let installed = ServerConfig::load_from(path).map_err(anyhow::Error::msg)?;
    if installed != *requested {
        bail!("{}", config_drift_message(input_path));
    }
    require_root_file(path, 0o644)
}

fn config_drift_message(input_path: &Path) -> String {
    let input_path = input_path.display();
    format!(
        "\
installed server config differs from install input

Installed runtime config: {SERVER_CONFIG_PATH}
Install input: {input_path}

{SERVER_CONFIG_PATH} is the runtime contract, materialized from install input.
The installer leaves a differing file in place.

Accidental change: re-run with the install input that produced the current server:
  scripts/install-server --config {input_path}

Intentional change: clear WT server state, then reinstall:
  make clear   # or: scripts/clear
  scripts/install-server --config {input_path}

`make clear` destroys every wt-* domain and removes installed WT state
(config, images, registry cache, worlds, client inventory under ~/.local/state/wt).
Packages and binaries stay installed."
    )
}

fn install_server_config(
    runner: &impl Runner,
    input_path: &Path,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    if Path::new(SERVER_CONFIG_PATH).exists() {
        return require_installed_config_compatible(input_path, server);
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
            cmd!("sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "0755", "/etc/wt",),
            "create /etc/wt",
        )?;
    }
    let local = Path::new("target").join("wt-server.toml.install");
    fs::write(&local, server_bytes).context("stage server config")?;
    let temporary = Path::new("/etc/wt/.server.toml.wt-new");
    if temporary.exists() {
        bail!("stale config install file exists: {}", temporary.display());
    }
    sudo_install(runner, &local, temporary, 0o644)?;
    sudo_move(runner, temporary, Path::new(SERVER_CONFIG_PATH))?;
    let _ = fs::remove_file(local);
    Ok(())
}

fn install_server_service(runner: &impl Runner, server: &ServerConfig) -> Result<()> {
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    let bytes = server_service(&user, server);
    let destination = Path::new(SERVER_SERVICE_PATH);
    if destination.exists() {
        require_root_file(destination, 0o644)?;
        if fs::read(destination).context("read installed wt-server service")? != bytes {
            bail!(
                "service unit drift at {SERVER_SERVICE_PATH}; remove it only when intentionally reinstalling the WT server"
            );
        }
    } else {
        let local = Path::new("target").join("wt-server.service.install");
        fs::write(&local, &bytes).context("stage wt-server service")?;
        let temporary = Path::new("/etc/systemd/system/.wt-server.service.wt-new");
        if temporary.exists() {
            bail!("stale service install file exists: {}", temporary.display());
        }
        sudo_install(runner, &local, temporary, 0o644)?;
        sudo_move(runner, temporary, destination)?;
        let _ = fs::remove_file(local);
    }
    runner.run(
        cmd!("sudo", "systemctl", "daemon-reload"),
        "reload systemd units",
    )?;
    runner.run(
        cmd!("sudo", "systemctl", "enable", "wt-server.service"),
        "enable wt-server service",
    )?;
    runner.run(
        cmd!("sudo", "systemctl", "restart", "wt-server.service"),
        "restart wt-server service",
    )
}

fn server_service(user: &User, server: &ServerConfig) -> Vec<u8> {
    let executable = server.install.binary_dir.join("wt-server");
    format!(
        "[Unit]\n\
Description=WT control-plane daemon\n\
Wants=network-online.target\n\
After=network-online.target docker.service libvirtd.service\n\
\n\
[Service]\n\
Type=simple\n\
User={}\n\
Environment={}\n\
ExecStart={} serve\n\
Restart=on-failure\n\
RuntimeDirectory=wt\n\
RuntimeDirectoryMode=0700\n\
UMask=0077\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        user.name,
        systemd_quote(&format!("HOME={}", user.dir.display())),
        systemd_quote(&executable.display().to_string()),
    )
    .into_bytes()
}

fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn config_drift_message_explains_recovery() {
        insta::assert_snapshot!(
            config_drift_message(Path::new("./server.toml")),
            @"
        installed server config differs from install input

        Installed runtime config: /etc/wt/server.toml
        Install input: ./server.toml

        /etc/wt/server.toml is the runtime contract, materialized from install input.
        The installer leaves a differing file in place.

        Accidental change: re-run with the install input that produced the current server:
          scripts/install-server --config ./server.toml

        Intentional change: clear WT server state, then reinstall:
          make clear   # or: scripts/clear
          scripts/install-server --config ./server.toml

        `make clear` destroys every wt-* domain and removes installed WT state
        (config, images, registry cache, worlds, client inventory under ~/.local/state/wt).
        Packages and binaries stay installed.
        "
        );
    }

    #[test]
    fn validates_server_owned_git_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let identity = temp.path().join("identity");
        let output = cmd!(
            "ssh-keygen",
            "-q",
            "-t",
            "ed25519",
            "-N",
            "secret",
            "-f",
            &identity,
        )
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

    #[test]
    fn service_runs_as_the_installing_user() {
        let user = User::from_uid(Uid::effective()).unwrap().unwrap();
        let server = toml::from_str::<InstallInput>(
            r#"
version = 1
[image]
source_url = "https://example.test/image"
source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
installed_path = "/var/lib/wt/image.qcow2"
[libvirt]
network = "default"
worlds_dir = "/var/lib/wt/worlds"
[registry_cache]
state_dir = "/var/lib/wt/cache"
port = 3128
max_size_gib = 1
registries = ["docker.io"]
[git]
identity_file = "~/.ssh/id"
known_hosts_file = "~/.ssh/known_hosts"
[guest]
session = "tmux"
memory_mib = 1024
vcpus = 1
disk_gib = 8
boot_timeout_seconds = 30
recipe_timeout_seconds = 30
ssh_authorized_keys_file = "~/.ssh/id.pub"
[install]
binary_dir = "/opt/wt bin"
"#,
        )
        .unwrap()
        .materialize();
        let unit = String::from_utf8(server_service(&user, &server)).unwrap();
        let unit = unit
            .replace(&user.dir.display().to_string(), "[HOME]")
            .replace(&user.name, "[USER]");
        insta::assert_snapshot!(unit, @r###"
        [Unit]
        Description=WT control-plane daemon
        Wants=network-online.target
        After=network-online.target docker.service libvirtd.service

        [Service]
        Type=simple
        User=[USER]
        Environment="HOME=[HOME]"
        ExecStart="/opt/wt bin/wt-server" serve
        Restart=on-failure
        RuntimeDirectory=wt
        RuntimeDirectoryMode=0700
        UMask=0077

        [Install]
        WantedBy=multi-user.target
        "###);
    }
}
