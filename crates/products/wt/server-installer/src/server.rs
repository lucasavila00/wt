mod binaries;

use crate::host;
use crate::image;
use crate::install_input::{
    serialize_capacity_config, serialize_server_config, AgentGitProviderInstallConfig, InstallInput,
};
use crate::registry_cache;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use std::fs;
use std::os::unix::fs::MetadataExt;
#[cfg(test)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use wt_installer_support::cmd;
#[cfg(test)]
use wt_installer_support::validate_passphrase;
use wt_installer_support::{
    prepare_ssh_credentials, read_owned_file, require_root_file, sudo_install, sudo_move,
    temporary_credential, validate_ssh_files, PassphrasePrompt, Runner, SshCredentialInput,
    TerminalPassphrasePrompt,
};
use wt_server::{ServerConfig, SERVER_CONFIG_PATH};
use wt_workload_registry::{CapacityConfig, CAPACITY_CONFIG_PATH};
#[cfg(test)]
use zeroize::Zeroizing;

const SERVER_SERVICE_PATH: &str = "/etc/systemd/system/wt-server.service";
const GATEWAY_SERVICE_PATH: &str = "/etc/systemd/system/wt-agent-git-gateway.service";
const CODEX_AUTH_SERVICE_PATH: &str = "/etc/systemd/system/wt-codex-integration-auth.service";
const CODEX_AUTH_PATH_UNIT_PATH: &str = "/etc/systemd/system/wt-codex-integration-auth.path";
const CODEX_AUTH_HELPER_PATH: &str = "/usr/local/libexec/wt-codex-integration-auth-share";
const CREDENTIAL_DIRECTORY: &str = "/etc/credstore.encrypted";
pub(crate) fn install(runner: &impl Runner, input_path: &Path) -> Result<()> {
    phase("Validating the installation");
    require_server_user()?;
    let (input, server, server_bytes) = load_install_input(input_path)?;
    require_workspace()?;
    require_installed_config_compatible(input_path, &server)?;
    require_installed_capacity_compatible(input_path, &input.capacity)?;
    let replace_runtime = !Path::new(SERVER_CONFIG_PATH).exists();

    phase("Preparing Git provider credentials");
    let prompt = TerminalPassphrasePrompt::new(ssh_key_passphrase_context);
    let credentials = prepare_agent_git_credentials(runner, &prompt, &input)?;

    phase("Preparing host state and caches");
    prepare_host(runner, &server)?;
    registry_cache::ensure(runner, &server)?;

    phase("Preparing the golden image");
    image::ensure(runner, &input, &server, &server_bytes)?;

    phase("Building and installing WT binaries");
    binaries::build_and_install(runner, &server)?;

    phase("Installing configuration and credentials");
    install_server_config(runner, input_path, &server, &server_bytes)?;
    install_capacity_config(runner, input_path, &input.capacity)?;
    install_agent_git_credentials(runner, &credentials)?;

    phase("Starting WT services");
    install_services(runner, &input, &server, replace_runtime)?;
    println!("\n{}", success_message(input_path));
    Ok(())
}

pub(crate) fn validate(input_path: &Path) -> Result<()> {
    let (input, _, _) = load_install_input(input_path)?;
    validate_agent_git_files(&input)
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
    println!(
        "images ready: {}, {}",
        server.image.devcontainer_path.display(),
        server.image.host_path.display()
    );
    Ok(())
}

pub(crate) fn verify_images(input_path: &Path) -> Result<()> {
    require_server_user()?;
    let (input, server, server_bytes) = load_install_input(input_path)?;
    require_workspace()?;
    image::verify(&input, &server, &server_bytes)
}

fn prepare_host(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    host::prepare_state(runner, config)
}

fn load_install_input(path: &Path) -> Result<(InstallInput, ServerConfig, Vec<u8>)> {
    let input = InstallInput::load_from(path).map_err(anyhow::Error::msg)?;
    let server = input.materialize();
    server.validate_codex_login().map_err(anyhow::Error::msg)?;
    let server_bytes = serialize_server_config(&server).map_err(anyhow::Error::msg)?;
    Ok((input, server, server_bytes))
}

struct PreparedProviderCredentials {
    kind: &'static str,
    api_token: tempfile::NamedTempFile,
    private_key: tempfile::NamedTempFile,
    known_hosts: tempfile::NamedTempFile,
}

fn validate_agent_git_files(input: &InstallInput) -> Result<()> {
    for (kind, provider) in input.agent_git.providers() {
        let token = read_owned_file(
            &provider.api_token_file,
            true,
            &format!("agent_git.{kind}.api_token_file"),
        )?;
        if token.iter().all(u8::is_ascii_whitespace) {
            bail!("agent_git.{kind}.api_token_file must not be empty");
        }
        validate_ssh_files(&provider_ssh_input(kind, provider))?;
    }
    Ok(())
}

fn prepare_agent_git_credentials(
    runner: &impl Runner,
    prompt: &impl PassphrasePrompt,
    input: &InstallInput,
) -> Result<Vec<PreparedProviderCredentials>> {
    validate_agent_git_files(input)?;
    input
        .agent_git
        .providers()
        .map(|(kind, provider)| prepare_provider_credentials(runner, prompt, kind, provider))
        .collect()
}

fn prepare_provider_credentials(
    runner: &impl Runner,
    prompt: &impl PassphrasePrompt,
    kind: &'static str,
    provider: &AgentGitProviderInstallConfig,
) -> Result<PreparedProviderCredentials> {
    let api_token = temporary_credential(&read_owned_file(
        &provider.api_token_file,
        true,
        &format!("agent_git.{kind}.api_token_file"),
    )?)?;
    let ssh = prepare_ssh_credentials(runner, prompt, &provider_ssh_input(kind, provider))?;
    Ok(PreparedProviderCredentials {
        kind,
        api_token,
        private_key: ssh.private_key,
        known_hosts: ssh.known_hosts,
    })
}

fn provider_ssh_input<'a>(
    kind: &'a str,
    provider: &'a AgentGitProviderInstallConfig,
) -> SshCredentialInput<'a> {
    SshCredentialInput {
        name: kind,
        host: &provider.host,
        private_key_file: &provider.ssh_private_key_file,
        public_key_file: Some(&provider.ssh_public_key_file),
        known_hosts_file: &provider.ssh_known_hosts_file,
    }
}

fn ssh_key_passphrase_context(kind: &str, path: &Path) -> String {
    let provider = match kind {
        "github" => "GitHub",
        "gitlab" => "GitLab",
        _ => kind,
    };
    format!(
        "{provider} SSH key is passphrase-protected\n\n\
Key: {}\n\
WT needs an unlocked copy so the local agent Git gateway can fetch and push.\n\
The original key will not be changed. WT verifies the key pair, encrypts the\n\
unlocked copy as a systemd credential, and removes the temporary copy.",
        path.display()
    )
}

fn phase(message: &str) {
    println!("\n{}", phase_message(message));
}

fn phase_message(message: &str) -> String {
    format!("==> {message}")
}

fn success_message(input_path: &Path) -> String {
    format!(
        "WT server is ready.\nConfig: {}\nServices started: wt-server, wt-agent-git-gateway\nNext: configure a WT client, then run `wt new`.",
        input_path.display()
    )
}

fn require_server_user() -> Result<()> {
    if Uid::effective().is_root() {
        bail!("run as the server user, not with sudo");
    }
    Ok(())
}

fn require_workspace() -> Result<()> {
    if !Path::new("Cargo.toml").is_file()
        || !Path::new("crates/products/wt/client/Cargo.toml").is_file()
        || !Path::new("crates/products/wt/devcontainer-guest-tools/Cargo.toml").is_file()
        || !Path::new("crates/products/wt/server/Cargo.toml").is_file()
    {
        bail!("run from the root of a wt source checkout");
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

fn require_installed_capacity_compatible(
    input_path: &Path,
    requested: &CapacityConfig,
) -> Result<()> {
    let path = Path::new(CAPACITY_CONFIG_PATH);
    if !path.exists() {
        return Ok(());
    }
    let installed = CapacityConfig::load_from(path).map_err(anyhow::Error::msg)?;
    if installed != *requested {
        bail!(
            "installed capacity config differs from {}; run make clear before reinstalling",
            input_path.display()
        );
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

`make clear` destroys every wt-* domain and removes generated runtime state
(config, golden image, worlds, grants, database, and generated SSH inventory).
It keeps installed services and credentials, source downloads, and caches."
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

fn install_capacity_config(
    runner: &impl Runner,
    input_path: &Path,
    capacity: &CapacityConfig,
) -> Result<()> {
    let path = Path::new(CAPACITY_CONFIG_PATH);
    if path.exists() {
        return require_installed_capacity_compatible(input_path, capacity);
    }
    let local = Path::new("target").join("wt-capacity.toml.install");
    let bytes = serialize_capacity_config(capacity).map_err(anyhow::Error::msg)?;
    fs::write(&local, bytes).context("stage capacity config")?;
    let temporary = Path::new("/etc/wt/.capacity.toml.wt-new");
    if temporary.exists() {
        bail!(
            "stale capacity config install file exists: {}",
            temporary.display()
        );
    }
    sudo_install(runner, &local, temporary, 0o644)?;
    sudo_move(runner, temporary, path)?;
    require_root_file(path, 0o644)
}

fn install_agent_git_credentials(
    runner: &impl Runner,
    providers: &[PreparedProviderCredentials],
) -> Result<()> {
    runner.run(
        cmd!(
            "sudo",
            "install",
            "-d",
            "-o",
            "root",
            "-g",
            "root",
            "-m",
            "0700",
            CREDENTIAL_DIRECTORY,
        ),
        "create encrypted credential directory",
    )?;
    runner.run(
        cmd!("sudo", "systemd-creds", "setup"),
        "initialize systemd credential encryption",
    )?;
    for provider in providers {
        for (suffix, source) in [
            ("api-token", provider.api_token.path()),
            ("ssh-private-key", provider.private_key.path()),
            ("ssh-known-hosts", provider.known_hosts.path()),
        ] {
            let credential = format!("{}-{suffix}", provider.kind);
            let destination =
                Path::new(CREDENTIAL_DIRECTORY).join(format!("wt-agent-git-gateway-{credential}"));
            let temporary = destination.with_extension("wt-new");
            if temporary.exists() {
                bail!(
                    "stale credential install file exists: {}",
                    temporary.display()
                );
            }
            runner.run(
                cmd!(
                    "sudo",
                    "systemd-creds",
                    "encrypt",
                    "--with-key=host",
                    format!("--name={credential}"),
                    source,
                    &temporary,
                ),
                &format!("encrypt {credential}"),
            )?;
            sudo_move(runner, &temporary, &destination)?;
        }
    }
    Ok(())
}

fn install_services(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    replace_runtime: bool,
) -> Result<()> {
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    install_codex_auth_helper(runner)?;
    install_service_unit(
        runner,
        "wt-codex-integration-auth",
        Path::new(CODEX_AUTH_SERVICE_PATH),
        &codex_auth_service(&user),
        replace_runtime,
    )?;
    install_service_unit(
        runner,
        "wt-codex-integration-auth-path",
        Path::new(CODEX_AUTH_PATH_UNIT_PATH),
        &codex_auth_path_unit(),
        replace_runtime,
    )?;
    install_service_unit(
        runner,
        "wt-agent-git-gateway",
        Path::new(GATEWAY_SERVICE_PATH),
        &gateway_service(&user, input, server),
        replace_runtime,
    )?;
    install_service_unit(
        runner,
        "wt-server",
        Path::new(SERVER_SERVICE_PATH),
        &server_service(&user, server),
        replace_runtime,
    )?;
    runner.run(
        cmd!("sudo", "systemctl", "daemon-reload"),
        "reload systemd units",
    )?;
    for name in [
        "wt-codex-integration-auth.path",
        "wt-agent-git-gateway.service",
        "wt-server.service",
    ] {
        runner.run(
            cmd!("sudo", "systemctl", "enable", name),
            &format!("enable {name}"),
        )?;
        runner.run(
            cmd!("sudo", "systemctl", "restart", name),
            &format!("restart {name}"),
        )?;
    }
    Ok(())
}

fn install_codex_auth_helper(runner: &impl Runner) -> Result<()> {
    runner.run(
        cmd!("sudo", "install", "-d", "-m", "0755", "/usr/local/libexec"),
        "create system helper directory",
    )?;
    let local = Path::new("target/wt-codex-integration-auth-share.install");
    fs::write(local, host::CODEX_AUTH_SHARE).context("stage Codex auth share helper")?;
    let temporary = Path::new("/usr/local/libexec/.wt-codex-integration-auth-share.wt-new");
    sudo_install(runner, local, temporary, 0o755)?;
    sudo_move(runner, temporary, Path::new(CODEX_AUTH_HELPER_PATH))?;
    let _ = fs::remove_file(local);
    Ok(())
}

fn install_service_unit(
    runner: &impl Runner,
    name: &str,
    destination: &Path,
    bytes: &[u8],
    replace_runtime: bool,
) -> Result<()> {
    if destination.exists() {
        require_root_file(destination, 0o644)?;
        let installed = fs::read(destination).context("read installed WT service")?;
        if !service_unit_needs_replacement(destination, &installed, bytes, replace_runtime)? {
            return Ok(());
        }
    }
    let local = Path::new("target").join(format!("{name}.service.install"));
    fs::write(&local, bytes).with_context(|| format!("stage {name} service"))?;
    let temporary = Path::new("/etc/systemd/system").join(format!(".{name}.service.wt-new"));
    if temporary.exists() {
        bail!("stale service install file exists: {}", temporary.display());
    }
    sudo_install(runner, &local, &temporary, 0o644)?;
    sudo_move(runner, &temporary, destination)?;
    let _ = fs::remove_file(local);
    Ok(())
}

fn service_unit_needs_replacement(
    destination: &Path,
    installed: &[u8],
    requested: &[u8],
    replace_runtime: bool,
) -> Result<bool> {
    if installed == requested {
        return Ok(false);
    }
    if replace_runtime {
        return Ok(true);
    }
    bail!(
        "service unit drift at {}; run make clear before reinstalling",
        destination.display()
    )
}

fn gateway_service(user: &User, input: &InstallInput, server: &ServerConfig) -> Vec<u8> {
    let executable = server.install.binary_dir.join("wt-agent-git-gateway");
    let mut command = format!("{} serve", systemd_quote(&executable.display().to_string()));
    let mut credentials = String::new();
    for (kind, provider) in input.agent_git.providers() {
        let token = format!("%d/{kind}-api-token");
        let key = format!("%d/{kind}-ssh-private-key");
        let known_hosts = format!("%d/{kind}-ssh-known-hosts");
        command.push_str(&format!(" --{kind}-provider "));
        command.push_str(&systemd_quote(&format!(
            "{}={token},{key},{known_hosts}",
            provider.host
        )));
        for suffix in ["api-token", "ssh-private-key", "ssh-known-hosts"] {
            let id = format!("{kind}-{suffix}");
            credentials.push_str(&format!(
                "LoadCredentialEncrypted={id}:{CREDENTIAL_DIRECTORY}/wt-agent-git-gateway-{id}\n"
            ));
        }
    }
    format!(
        "[Unit]\n\
Description=WT agent Git gateway\n\
Wants=network-online.target\n\
After=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
User={}\n\
Environment={}\n\
Environment={}\n\
{}\n\
ExecStart={}\n\
Restart=on-failure\n\
RuntimeDirectory=wt-agent-git-gateway\n\
RuntimeDirectoryMode=0700\n\
StateDirectory=wt/agent-git\n\
StateDirectoryMode=0700\n\
UMask=0077\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        user.name,
        systemd_quote(&format!("HOME={}", user.dir.display())),
        systemd_quote(&format!(
            "{}={}",
            wt_server::AGENT_GIT_VSOCK_PORT_ENV,
            server.agent_git.vsock_port
        )),
        credentials.trim_end(),
        command,
    )
    .into_bytes()
}

fn server_service(user: &User, server: &ServerConfig) -> Vec<u8> {
    let executable = server.install.binary_dir.join("wt-server");
    format!(
        "[Unit]\n\
Description=WT control-plane daemon\n\
Requires=wt-codex-integration-auth.service\n\
Wants=network-online.target wt-agent-git-gateway.service wt-codex-integration-auth.path\n\
After=network-online.target docker.service libvirtd.service wt-agent-git-gateway.service wt-codex-integration-auth.service\n\
\n\
[Service]\n\
Type=simple\n\
User={}\n\
Environment={}\n\
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
        systemd_quote(&format!(
            "{}={}",
            wt_server::AGENT_GIT_VSOCK_PORT_ENV,
            server.agent_git.vsock_port
        )),
        systemd_quote(&executable.display().to_string()),
    )
    .into_bytes()
}

fn codex_auth_service(user: &User) -> Vec<u8> {
    format!(
        "[Unit]\n\
Description=Refresh the WT Codex authentication share\n\
\n\
[Service]\n\
Type=oneshot\n\
Environment={}\n\
ExecStart={}\n\
UMask=0077\n",
        systemd_quote(&format!("HOME={}", user.dir.display())),
        CODEX_AUTH_HELPER_PATH,
    )
    .into_bytes()
}

fn codex_auth_path_unit() -> Vec<u8> {
    format!(
        "[Unit]\n\
Description=Watch the WT Codex authentication file\n\
\n\
[Path]\n\
PathChanged={}\n\
Unit=wt-codex-integration-auth.service\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        wt_server::CODEX_AUTH_PATH,
    )
    .into_bytes()
}

fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests;
