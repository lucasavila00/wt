use crate::files::{require_root_file, sudo_install, sudo_move};
use crate::host;
use crate::image;
use crate::install_input::{serialize_server_config, AgentGitProviderInstallConfig, InstallInput};
use crate::registry_cache;
use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use wt_command::cmd;
use wt_server::{ServerConfig, SERVER_CONFIG_PATH};
use zeroize::Zeroizing;

const SERVER_SERVICE_PATH: &str = "/etc/systemd/system/wt-server.service";
const GATEWAY_SERVICE_PATH: &str = "/etc/systemd/system/wt-agent-git-gateway.service";
const CREDENTIAL_DIRECTORY: &str = "/etc/credstore.encrypted";

pub(crate) fn install(runner: &impl Runner, input_path: &Path) -> Result<()> {
    phase("Validating the installation");
    require_server_user()?;
    let (input, server, server_bytes) = load_install_input(input_path)?;
    require_workspace()?;
    require_installed_config_compatible(input_path, &server)?;
    let replace_runtime = !Path::new(SERVER_CONFIG_PATH).exists();

    phase("Preparing Git provider credentials");
    let credentials = prepare_agent_git_credentials(runner, &TerminalPassphrasePrompt, &input)?;

    phase("Preparing host state and caches");
    prepare_host(runner, &server)?;
    registry_cache::ensure(runner, &server)?;

    phase("Preparing the golden image");
    image::ensure(runner, &input, &server, &server_bytes)?;

    phase("Building and installing WT binaries");
    build_and_install_binaries(runner, &server)?;

    phase("Installing configuration and credentials");
    install_server_config(runner, input_path, &server, &server_bytes)?;
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
    println!("image ready: {}", server.image.installed_path.display());
    Ok(())
}

fn prepare_host(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    host::prepare_state(runner, config)
}

fn load_install_input(path: &Path) -> Result<(InstallInput, ServerConfig, Vec<u8>)> {
    let input = InstallInput::load_from(path).map_err(anyhow::Error::msg)?;
    let server = input.materialize();
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
        read_owned_file(
            &provider.ssh_private_key_file,
            true,
            &format!("agent_git.{kind}.ssh_private_key_file"),
        )?;
        read_owned_file(
            &provider.ssh_public_key_file,
            false,
            &format!("agent_git.{kind}.ssh_public_key_file"),
        )?;
        read_owned_file(
            &provider.ssh_known_hosts_file,
            false,
            &format!("agent_git.{kind}.ssh_known_hosts_file"),
        )?;
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
    let private_key_bytes = read_owned_file(
        &provider.ssh_private_key_file,
        true,
        &format!("agent_git.{kind}.ssh_private_key_file"),
    )?;
    let private_key = ssh_key::PrivateKey::from_openssh(&private_key_bytes).with_context(|| {
        format!(
            "parse {kind} SSH private key {}",
            provider.ssh_private_key_file.display()
        )
    })?;
    let private_key = if private_key.is_encrypted() {
        let passphrase = prompt.read(kind, &provider.ssh_private_key_file, &private_key)?;
        private_key.decrypt(&passphrase).with_context(|| {
            format!(
                "unlock {kind} SSH private key {}",
                provider.ssh_private_key_file.display()
            )
        })?
    } else {
        private_key
    };
    let unlocked = private_key
        .to_openssh(ssh_key::LineEnding::LF)
        .with_context(|| format!("encode unlocked {kind} SSH private key"))?;
    let private_key_file = temporary_credential(unlocked.as_bytes())?;
    let derived_public = private_key
        .public_key()
        .to_openssh()
        .with_context(|| format!("encode {kind} SSH public key"))?;
    let configured_public = String::from_utf8(read_owned_file(
        &provider.ssh_public_key_file,
        false,
        &format!("agent_git.{kind}.ssh_public_key_file"),
    )?)
    .with_context(|| format!("decode agent_git.{kind}.ssh_public_key_file"))?;
    let derived_public =
        public_key_fields(&derived_public).context("ssh-keygen returned an invalid public key")?;
    let configured_public = public_key_fields(&configured_public)
        .with_context(|| format!("agent_git.{kind}.ssh_public_key_file is invalid"))?;
    if derived_public != configured_public {
        bail!("agent_git.{kind} SSH public key does not match its private key");
    }
    let known_hosts = temporary_credential(&read_owned_file(
        &provider.ssh_known_hosts_file,
        false,
        &format!("agent_git.{kind}.ssh_known_hosts_file"),
    )?)?;
    let output = runner.output(cmd!(
        "ssh-keygen",
        "-F",
        &provider.host,
        "-f",
        known_hosts.path(),
    ))?;
    if !output.status.success() || output.stdout.is_empty() {
        bail!(
            "agent_git.{kind}.ssh_known_hosts_file has no key for {}",
            provider.host
        );
    }
    Ok(PreparedProviderCredentials {
        kind,
        api_token,
        private_key: private_key_file,
        known_hosts,
    })
}

trait PassphrasePrompt {
    fn read(
        &self,
        kind: &str,
        path: &Path,
        private_key: &ssh_key::PrivateKey,
    ) -> Result<Zeroizing<String>>;
}

struct TerminalPassphrasePrompt;

impl PassphrasePrompt for TerminalPassphrasePrompt {
    fn read(
        &self,
        kind: &str,
        path: &Path,
        private_key: &ssh_key::PrivateKey,
    ) -> Result<Zeroizing<String>> {
        eprintln!("\n{}", ssh_key_passphrase_context(kind, path));
        let key = private_key.clone();
        let passphrase = cliclack::password(format!("Passphrase for {}", path.display()))
            .validate(move |value: &String| validate_passphrase(&key, value))
            .interact()
            .with_context(|| format!("read {kind} SSH key passphrase"))?;
        Ok(Zeroizing::new(passphrase))
    }
}

fn validate_passphrase(
    private_key: &ssh_key::PrivateKey,
    passphrase: &str,
) -> std::result::Result<(), &'static str> {
    private_key
        .decrypt(passphrase)
        .map(|_| ())
        .map_err(|_| "That passphrase did not unlock this SSH key. Try again.")
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

fn read_owned_file(path: &Path, private: bool, name: &str) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {name} {}", path.display()))?;
    let mode = metadata.mode() & 0o7777;
    let valid_mode = mode == 0o600 || (!private && mode == 0o644);
    if !metadata.file_type().is_file() || metadata.uid() != Uid::effective().as_raw() || !valid_mode
    {
        let expected = if private { "0600" } else { "0600 or 0644" };
        bail!(
            "{name} {} must be a regular file owned by the installing user with mode {expected}",
            path.display()
        );
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {name} {}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {name} {}", path.display()))?;
    Ok(bytes)
}

fn temporary_credential(bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new().context("create temporary credential")?;
    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600))
        .context("protect temporary credential")?;
    file.write_all(bytes)
        .context("write temporary credential")?;
    file.flush().context("flush temporary credential")?;
    Ok(file)
}

fn public_key_fields(value: &str) -> Option<(&str, &str)> {
    let mut fields = value.split_whitespace();
    Some((fields.next()?, fields.next()?))
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
            "--quiet",
            "--release",
            "-p",
            "wt-agent-git",
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
        "wt-agent-git-gateway",
        "wt-agent-git-relay",
        "git-remote-ag",
        "ag-git",
        "wt",
        "wt-app-pane",
        "wt-app-info",
        "wt-app-proxy",
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
                Path::new(CREDENTIAL_DIRECTORY).join(format!("wt-agent-git-{credential}"));
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
    for name in ["wt-agent-git-gateway.service", "wt-server.service"] {
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
                "LoadCredentialEncrypted={id}:{CREDENTIAL_DIRECTORY}/wt-agent-git-{id}\n"
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
{}\n\
ExecStart={}\n\
Restart=on-failure\n\
RuntimeDirectory=wt-agent-git\n\
RuntimeDirectoryMode=0700\n\
StateDirectory=wt/agent-git\n\
StateDirectoryMode=0700\n\
UMask=0077\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        user.name,
        systemd_quote(&format!("HOME={}", user.dir.display())),
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
Wants=network-online.target wt-agent-git-gateway.service\n\
After=network-online.target docker.service libvirtd.service wt-agent-git-gateway.service\n\
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
mod tests;
