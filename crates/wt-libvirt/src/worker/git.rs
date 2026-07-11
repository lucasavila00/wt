//! SSH-only Git provisioning and the credential bridge into the devcontainer.

use super::guest_agent;
use crate::WorkerError;
use nix::unistd::Uid;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use virt::domain::Domain;

const BUNDLE_DIR: &str = "/workspace/.git/wt";
const CLONE_CREDENTIALS_DIR: &str = "/run/wt-git";
const SSH_COMMAND: &str = "sh -c 'exec \"$(git rev-parse --git-common-dir)/wt/ssh\" \"$@\"' wt-ssh";
const SSH_WRAPPER: &[u8] = br#"#!/bin/sh
set -eu
directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
runtime=$(mktemp -d "${TMPDIR:-/tmp}/wt-git.XXXXXX")
trap 'rm -rf "$runtime"' EXIT HUP INT TERM
install -m 0600 "$directory/identity" "$runtime/identity"
/usr/bin/ssh \
  -i "$runtime/identity" \
  -o IdentitiesOnly=yes \
  -o UserKnownHostsFile="$directory/known_hosts" \
  -o StrictHostKeyChecking=yes \
  "$@"
"#;

pub(super) struct Credentials {
    identity: Vec<u8>,
    known_hosts: Vec<u8>,
}

pub(super) fn load_credentials(
    identity_file: &Path,
    known_hosts_file: &Path,
) -> Result<Credentials, WorkerError> {
    let metadata = fs::metadata(identity_file)
        .map_err(|error| error_context("Git identity: inspect private key", error))?;
    if !metadata.is_file()
        || metadata.uid() != Uid::effective().as_raw()
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(WorkerError::new(
            "Git identity: configured server key must be a regular file owned by the site user with mode 0600",
        ));
    }
    let identity = fs::read(identity_file)
        .map_err(|error| error_context("Git identity: read private key", error))?;
    let identity_path = identity_file.display().to_string();
    if !private_key_accepts_passphrase(&identity_path, "")? {
        return Err(WorkerError::new(
            "Git identity: configured server key must be unencrypted",
        ));
    }
    let known_hosts = fs::read(known_hosts_file).map_err(|error| {
        error_context("Git host trust: read configured known-hosts file", error)
    })?;
    Ok(Credentials {
        identity,
        known_hosts,
    })
}

pub(super) fn clone_and_checkout(
    domain: &Domain,
    source: &str,
    git_ref: Option<&str>,
    credentials: &Credentials,
    deadline: Instant,
) -> Result<(), WorkerError> {
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/usr/bin/install",
        &["-d", "-m", "0700", CLONE_CREDENTIALS_DIR],
        deadline,
    )?;
    let result = (|| {
        stage_clone_credentials(domain, credentials, deadline)?;
        let environment = git_environment();
        run_git(
            domain,
            "Git clone",
            &environment,
            &["clone", "--", source, "/workspace"],
            deadline,
        )?;
        if let Some(git_ref) = git_ref {
            run_git(
                domain,
                "Git fetch ref",
                &environment,
                &["-C", "/workspace", "fetch", "origin", git_ref],
                deadline,
            )?;
            run_git(
                domain,
                "Git checkout ref",
                &environment,
                &["-C", "/workspace", "checkout", "--detach", "FETCH_HEAD"],
                deadline,
            )?;
        }
        install_persistent_bundle(domain, credentials, deadline)
    })();
    let _ = guest_agent::exec(domain, "/bin/rm", &["-rf", CLONE_CREDENTIALS_DIR], deadline);
    result
}

fn stage_clone_credentials(
    domain: &Domain,
    credentials: &Credentials,
    deadline: Instant,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, "/run/wt-git/identity", &credentials.identity)?;
    guest_agent::write(domain, "/run/wt-git/known_hosts", &credentials.known_hosts)?;
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/bin/chmod",
        &["0600", "/run/wt-git/identity", "/run/wt-git/known_hosts"],
        deadline,
    )?;
    Ok(())
}

fn git_environment() -> Vec<String> {
    vec![
        "GIT_SSH_COMMAND=ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes".to_owned(),
    ]
}

fn run_git(
    domain: &Domain,
    phase: &str,
    environment: &[String],
    git_args: &[&str],
    deadline: Instant,
) -> Result<(), WorkerError> {
    let mut args = environment.iter().map(String::as_str).collect::<Vec<_>>();
    args.push("/usr/bin/git");
    // cloud-init creates /workspace for wt, while guest-agent provisioning runs as root.
    args.extend(["-c", "safe.directory=/workspace"]);
    args.extend_from_slice(git_args);
    guest_agent::run_phase(domain, phase, "/usr/bin/env", &args, deadline)?;
    Ok(())
}

fn install_persistent_bundle(
    domain: &Domain,
    credentials: &Credentials,
    deadline: Instant,
) -> Result<(), WorkerError> {
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/usr/bin/install",
        &["-d", "-m", "0755", BUNDLE_DIR],
        deadline,
    )?;
    guest_agent::write(
        domain,
        &format!("{BUNDLE_DIR}/identity"),
        &credentials.identity,
    )?;
    guest_agent::write(
        domain,
        &format!("{BUNDLE_DIR}/known_hosts"),
        &credentials.known_hosts,
    )?;
    guest_agent::write(domain, &format!("{BUNDLE_DIR}/ssh"), SSH_WRAPPER)?;

    // The bundle is intentionally visible to the trusted devcontainer. The
    // wrapper gives OpenSSH a private mode-0600 copy for each invocation.
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/bin/chmod",
        &[
            "0444",
            &format!("{BUNDLE_DIR}/identity"),
            &format!("{BUNDLE_DIR}/known_hosts"),
        ],
        deadline,
    )?;
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/bin/chmod",
        &["0555", &format!("{BUNDLE_DIR}/ssh")],
        deadline,
    )?;
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/usr/bin/git",
        &[
            "-c",
            "safe.directory=/workspace",
            "-C",
            "/workspace",
            "config",
            "--local",
            "core.sshCommand",
            SSH_COMMAND,
        ],
        deadline,
    )?;
    Ok(())
}

fn private_key_accepts_passphrase(path: &str, passphrase: &str) -> Result<bool, WorkerError> {
    Ok(Command::new("ssh-keygen")
        .args(["-y", "-P", passphrase, "-f", path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| error_context("Git identity: inspect private key", error))?
        .success())
}

fn error_context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn persistent_ssh_command_resolves_from_nested_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        fs::create_dir(&repository).unwrap();
        run(
            Command::new("git").args(["init", "-q"]).arg(&repository),
            "initialize test repository",
        );
        let bundle = repository.join(".git/wt");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("identity"), "not-read-by-ssh-g\n").unwrap();
        fs::write(bundle.join("known_hosts"), "").unwrap();
        fs::write(bundle.join("ssh"), SSH_WRAPPER).unwrap();
        fs::set_permissions(bundle.join("ssh"), fs::Permissions::from_mode(0o555)).unwrap();
        run(
            Command::new("git").args(["-C"]).arg(&repository).args([
                "config",
                "core.sshCommand",
                SSH_COMMAND,
            ]),
            "configure test SSH command",
        );
        let nested = repository.join("nested");
        fs::create_dir(&nested).unwrap();
        let runtime = temp.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{SSH_COMMAND} -T -G example.test >/dev/null"))
            .current_dir(&nested)
            .env("TMPDIR", &runtime)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_dir(runtime).unwrap().count(), 0);
    }

    #[test]
    fn detects_encrypted_private_key_passphrases() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("identity");
        run(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "secret", "-f"])
                .arg(&key),
            "generate encrypted key",
        );
        let key = key.to_str().unwrap();
        assert!(!private_key_accepts_passphrase(key, "").unwrap());
        assert!(!private_key_accepts_passphrase(key, "wrong").unwrap());
        assert!(private_key_accepts_passphrase(key, "secret").unwrap());
    }

    fn run(command: &mut Command, action: &str) {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
