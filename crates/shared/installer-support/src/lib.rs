//! Shared, security-sensitive setup helpers used by WT installers.

use anyhow::{bail, Context, Result};
use nix::unistd::{Group, Uid, User};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use wt_command::cmd;
use zeroize::Zeroizing;

pub trait Runner {
    fn output(&self, command: Command) -> Result<Output>;

    fn run(&self, command: Command, action: &str) -> Result<()> {
        let output = self.output(command)?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn text(&self, command: Command, action: &str) -> Result<String> {
        let output = self.output(command)?;
        if !output.status.success() {
            bail!(
                "{action}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        String::from_utf8(output.stdout).with_context(|| format!("decode output from {action}"))
    }

    fn run_script(&self, script: &[u8], args: &[&str], action: &str) -> Result<()> {
        let mut child = Command::new("/bin/sh")
            .args(["-s", "--"])
            .args(args)
            .stdin(Stdio::piped())
            .spawn()
            .context("start /bin/sh")?;
        child
            .stdin
            .take()
            .context("open /bin/sh stdin")?
            .write_all(script)
            .context("write shell script")?;
        let status = child.wait().context("wait for /bin/sh")?;
        if !status.success() {
            bail!("{action}: script exited with {status}");
        }
        Ok(())
    }
}

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn output(&self, mut command: Command) -> Result<Output> {
        let program = command.get_program().to_string_lossy().into_owned();
        command.output().with_context(|| format!("run {program}"))
    }

    fn run(&self, mut command: Command, action: &str) -> Result<()> {
        let program = command.get_program().to_string_lossy().into_owned();
        let status = command.status().with_context(|| format!("run {program}"))?;
        if !status.success() {
            bail!("{action}: command exited with {status}");
        }
        Ok(())
    }
}

pub fn require_root_file(path: &Path, mode: u32) -> Result<()> {
    require_named_file(path, "root", "root", mode)
}

pub fn require_named_file(path: &Path, owner: &str, group: &str, mode: u32) -> Result<()> {
    let uid = User::from_name(owner)
        .with_context(|| format!("look up user {owner}"))?
        .with_context(|| format!("required user does not exist: {owner}"))?
        .uid;
    let gid = Group::from_name(group)
        .with_context(|| format!("look up group {group}"))?
        .with_context(|| format!("required group does not exist: {group}"))?
        .gid;
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if !metadata.is_file()
        || metadata.uid() != uid.as_raw()
        || metadata.gid() != gid.as_raw()
        || metadata.mode() & 0o7777 != mode
    {
        bail!(
            "file drift at {}: expected {owner}:{group}, mode={mode:04o}",
            path.display(),
        );
    }
    Ok(())
}

pub fn sudo_install(
    runner: &impl Runner,
    source: &Path,
    destination: &Path,
    mode: u32,
) -> Result<()> {
    sudo_install_owned(runner, source, destination, "root", "root", mode)
}

pub fn sudo_install_owned(
    runner: &impl Runner,
    source: &Path,
    destination: &Path,
    owner: &str,
    group: &str,
    mode: u32,
) -> Result<()> {
    runner.run(
        cmd!(
            "sudo",
            "install",
            "-o",
            owner,
            "-g",
            group,
            "-m",
            format!("{mode:04o}"),
            source,
            destination,
        ),
        "install file",
    )
}

pub fn sudo_move(runner: &impl Runner, source: &Path, destination: &Path) -> Result<()> {
    runner.run(
        cmd!("sudo", "mv", "--", source, destination),
        "publish installed file",
    )
}

pub fn expand_home(path: &Path, name: &str) -> Result<PathBuf, String> {
    if path == Path::new("~") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned());
    }
    if let Some(relative) = path.to_str().and_then(|value| value.strip_prefix("~/")) {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        return Ok(home.join(relative));
    }
    if !path.is_absolute() {
        return Err(format!("{name} must be absolute or start with ~/"));
    }
    Ok(path.to_owned())
}

pub fn read_owned_file(path: &Path, private: bool, name: &str) -> Result<Vec<u8>> {
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

pub fn temporary_credential(bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new().context("create temporary credential")?;
    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600))
        .context("protect temporary credential")?;
    file.write_all(bytes)
        .context("write temporary credential")?;
    file.flush().context("flush temporary credential")?;
    Ok(file)
}

pub struct SshCredentialInput<'a> {
    pub name: &'a str,
    pub host: &'a str,
    pub private_key_file: &'a Path,
    pub public_key_file: Option<&'a Path>,
    pub known_hosts_file: &'a Path,
}

pub struct PreparedSshCredentials {
    pub private_key: tempfile::NamedTempFile,
    pub known_hosts: tempfile::NamedTempFile,
}

pub trait PassphrasePrompt {
    fn read(
        &self,
        name: &str,
        path: &Path,
        private_key: &ssh_key::PrivateKey,
    ) -> Result<Zeroizing<String>>;
}

pub struct TerminalPassphrasePrompt<F> {
    context: F,
}

impl<F> TerminalPassphrasePrompt<F> {
    pub fn new(context: F) -> Self {
        Self { context }
    }
}

impl<F> PassphrasePrompt for TerminalPassphrasePrompt<F>
where
    F: Fn(&str, &Path) -> String,
{
    fn read(
        &self,
        name: &str,
        path: &Path,
        private_key: &ssh_key::PrivateKey,
    ) -> Result<Zeroizing<String>> {
        eprintln!("\n{}", (self.context)(name, path));
        let key = private_key.clone();
        let passphrase = cliclack::password(format!("Passphrase for {}", path.display()))
            .validate(move |value: &String| validate_passphrase(&key, value))
            .interact()
            .with_context(|| format!("read {name} SSH key passphrase"))?;
        Ok(Zeroizing::new(passphrase))
    }
}

pub fn validate_ssh_files(input: &SshCredentialInput<'_>) -> Result<()> {
    read_owned_file(input.private_key_file, true, &private_key_name(input))?;
    if let Some(public_key_file) = input.public_key_file {
        read_owned_file(public_key_file, false, &public_key_name(input))?;
    }
    read_owned_file(input.known_hosts_file, false, &known_hosts_name(input))?;
    Ok(())
}

pub fn prepare_ssh_credentials(
    runner: &impl Runner,
    prompt: &impl PassphrasePrompt,
    input: &SshCredentialInput<'_>,
) -> Result<PreparedSshCredentials> {
    validate_ssh_files(input)?;
    let private_key_bytes =
        read_owned_file(input.private_key_file, true, &private_key_name(input))?;
    let private_key = ssh_key::PrivateKey::from_openssh(&private_key_bytes).with_context(|| {
        format!(
            "parse {} SSH private key {}",
            input.name,
            input.private_key_file.display()
        )
    })?;
    let private_key = if private_key.is_encrypted() {
        let passphrase = prompt.read(input.name, input.private_key_file, &private_key)?;
        private_key.decrypt(&passphrase).with_context(|| {
            format!(
                "unlock {} SSH private key {}",
                input.name,
                input.private_key_file.display()
            )
        })?
    } else {
        private_key
    };
    let unlocked = private_key
        .to_openssh(ssh_key::LineEnding::LF)
        .with_context(|| format!("encode unlocked {} SSH private key", input.name))?;
    let private_key_file = temporary_credential(unlocked.as_bytes())?;

    if let Some(public_key_file) = input.public_key_file {
        let derived_public = private_key
            .public_key()
            .to_openssh()
            .with_context(|| format!("encode {} SSH public key", input.name))?;
        let configured_public = String::from_utf8(read_owned_file(
            public_key_file,
            false,
            &public_key_name(input),
        )?)
        .with_context(|| format!("decode {}", public_key_name(input)))?;
        let derived_public =
            public_key_fields(&derived_public).context("derived SSH public key is invalid")?;
        let configured_public = public_key_fields(&configured_public)
            .with_context(|| format!("{} is invalid", public_key_name(input)))?;
        if derived_public != configured_public {
            bail!(
                "{} SSH public key does not match its private key",
                input.name
            );
        }
    }

    let known_hosts = temporary_credential(&read_owned_file(
        input.known_hosts_file,
        false,
        &known_hosts_name(input),
    )?)?;
    let output = runner.output(cmd!(
        "ssh-keygen",
        "-F",
        input.host,
        "-f",
        known_hosts.path(),
    ))?;
    if !output.status.success() || output.stdout.is_empty() {
        bail!("{} has no key for {}", known_hosts_name(input), input.host);
    }
    Ok(PreparedSshCredentials {
        private_key: private_key_file,
        known_hosts,
    })
}

pub fn validate_passphrase(
    private_key: &ssh_key::PrivateKey,
    passphrase: &str,
) -> std::result::Result<(), &'static str> {
    private_key
        .decrypt(passphrase)
        .map(|_| ())
        .map_err(|_| "That passphrase did not unlock this SSH key. Try again.")
}

fn private_key_name(input: &SshCredentialInput<'_>) -> String {
    format!("{}.ssh_private_key_file", input.name)
}

fn public_key_name(input: &SshCredentialInput<'_>) -> String {
    format!("{}.ssh_public_key_file", input.name)
}

fn known_hosts_name(input: &SshCredentialInput<'_>) -> String {
    format!("{}.ssh_known_hosts_file", input.name)
}

fn public_key_fields(value: &str) -> Option<(&str, &str)> {
    let mut fields = value.split_whitespace();
    Some((fields.next()?, fields.next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_paths_are_expanded() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(
            expand_home(Path::new("~/key"), "key").unwrap(),
            home.join("key")
        );
        assert!(expand_home(Path::new("relative"), "key").is_err());
    }
}
