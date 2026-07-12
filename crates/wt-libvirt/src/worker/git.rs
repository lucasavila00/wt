//! SSH-only Git provisioning and the credential bridge into the devcontainer.

use super::guest_agent;
use crate::WorkerError;
use nix::unistd::Uid;
use ssh_key::PrivateKey;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;
use virt::domain::Domain;
use wt_api::GitPassphrase;

const BUNDLE_DIR: &str = "/workspace/.git/wt";
const CLONE_CREDENTIALS_DIR: &str = "/run/wt-git";
const CLONE_ASKPASS: &str = "/tmp/wt-git-askpass";
const SSH_COMMAND: &str = "sh -c 'exec \"$(git rev-parse --git-common-dir)/wt/ssh\" \"$@\"' wt-ssh";
const STAGE_CREDENTIAL_MODES: &str = "chmod 0700 \"$1\" && chmod 0600 \"$2\" \"$3\" \"$4\"";
const FINALIZE_BUNDLE: &str = "chmod 0444 \"$1\" \"$2\" && chmod 0555 \"$3\" && exec /usr/bin/git -c safe.directory=/workspace -C /workspace config --local core.sshCommand \"$4\"";
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

#[derive(Clone)]
pub(super) struct Credentials {
    identity: Vec<u8>,
    private_key: PrivateKey,
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
            "Git identity: configured server key must be a regular file owned by the server user with mode 0600",
        ));
    }
    let identity = fs::read(identity_file)
        .map_err(|error| error_context("Git identity: read private key", error))?;
    let encoded = std::str::from_utf8(&identity)
        .map_err(|error| error_context("Git identity: decode private key", error))?;
    let private_key = PrivateKey::from_openssh(encoded)
        .map_err(|error| error_context("Git identity: parse private key", error))?;
    if !private_key.is_encrypted() {
        return Err(WorkerError::new(
            "Git identity: configured server key must be encrypted",
        ));
    }
    let known_hosts = fs::read(known_hosts_file).map_err(|error| {
        error_context("Git host trust: read configured known-hosts file", error)
    })?;
    Ok(Credentials {
        identity,
        private_key,
        known_hosts,
    })
}

impl Credentials {
    pub(super) fn validate_passphrase(
        &self,
        passphrase: &GitPassphrase,
    ) -> Result<(), WorkerError> {
        self.private_key
            .decrypt(passphrase.expose_secret())
            .map(|_| ())
            .map_err(|_| WorkerError::new("Git identity: invalid private key passphrase"))
    }
}

pub(super) fn clone_and_checkout(
    domain: &Domain,
    source: &str,
    credentials: &Credentials,
    passphrase: &GitPassphrase,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    credentials.validate_passphrase(passphrase)?;
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/usr/bin/install",
        &["-d", "-m", "0700", CLONE_CREDENTIALS_DIR],
        deadline,
        log,
    )?;
    let result = (|| {
        stage_clone_credentials(domain, credentials, passphrase, deadline, log)?;
        let environment = git_environment();
        run_git(
            domain,
            "Git clone",
            &environment,
            &["clone", "--", source, "/workspace"],
            deadline,
            log,
        )?;
        install_persistent_bundle(domain, credentials, deadline, log)
    })();
    let _ = guest_agent::exec(
        domain,
        "/bin/rm",
        &["-rf", CLONE_CREDENTIALS_DIR, CLONE_ASKPASS],
        deadline,
    );
    result
}

pub(super) fn configure_author(
    domain: &Domain,
    name: Option<&str>,
    email: Option<&str>,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    for (key, value) in [("user.name", name), ("user.email", email)] {
        let Some(value) = value else {
            continue;
        };
        run_git(
            domain,
            "Git author identity",
            &[],
            &["-C", "/workspace", "config", "--local", key, value],
            deadline,
            log,
        )?;
    }
    Ok(())
}

fn stage_clone_credentials(
    domain: &Domain,
    credentials: &Credentials,
    passphrase: &GitPassphrase,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, "/run/wt-git/identity", &credentials.identity)?;
    guest_agent::write(domain, "/run/wt-git/known_hosts", &credentials.known_hosts)?;
    guest_agent::write(
        domain,
        "/run/wt-git/passphrase",
        passphrase.expose_secret().as_bytes(),
    )?;
    guest_agent::write(
        domain,
        CLONE_ASKPASS,
        b"#!/bin/sh\ncat /run/wt-git/passphrase\n",
    )?;
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/bin/sh",
        &[
            "-c",
            STAGE_CREDENTIAL_MODES,
            "wt-git-credentials",
            CLONE_ASKPASS,
            "/run/wt-git/identity",
            "/run/wt-git/known_hosts",
            "/run/wt-git/passphrase",
        ],
        deadline,
        log,
    )?;
    Ok(())
}

fn git_environment() -> Vec<String> {
    vec![
        "GIT_SSH_COMMAND=ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes".to_owned(),
        format!("SSH_ASKPASS={CLONE_ASKPASS}"),
        "SSH_ASKPASS_REQUIRE=force".to_owned(),
        "DISPLAY=wt:0".to_owned(),
    ]
}

fn run_git(
    domain: &Domain,
    phase: &str,
    environment: &[String],
    git_args: &[&str],
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    let mut args = environment.iter().map(String::as_str).collect::<Vec<_>>();
    args.push("/usr/bin/git");
    // cloud-init creates /workspace for wt, while guest-agent provisioning runs as root.
    args.extend(["-c", "safe.directory=/workspace"]);
    args.extend_from_slice(git_args);
    guest_agent::run_phase(domain, phase, "/usr/bin/env", &args, deadline, log)?;
    Ok(())
}

fn install_persistent_bundle(
    domain: &Domain,
    credentials: &Credentials,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::run_phase(
        domain,
        "Git credentials",
        "/usr/bin/install",
        &["-d", "-m", "0755", BUNDLE_DIR],
        deadline,
        log,
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
        "/bin/sh",
        &[
            "-c",
            FINALIZE_BUNDLE,
            "wt-git-bundle",
            &format!("{BUNDLE_DIR}/identity"),
            &format!("{BUNDLE_DIR}/known_hosts"),
            &format!("{BUNDLE_DIR}/ssh"),
            SSH_COMMAND,
        ],
        deadline,
        log,
    )?;
    Ok(())
}

fn error_context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use wt_command::cmd;

    #[test]
    fn persistent_ssh_command_resolves_from_nested_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        fs::create_dir(&repository).unwrap();
        run(
            cmd!("git", "init", "-q", &repository),
            "initialize test repository",
        );
        let bundle = repository.join(".git/wt");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("identity"), "not-read-by-ssh-g\n").unwrap();
        fs::write(bundle.join("known_hosts"), "").unwrap();
        fs::write(bundle.join("ssh"), SSH_WRAPPER).unwrap();
        fs::set_permissions(bundle.join("ssh"), fs::Permissions::from_mode(0o555)).unwrap();
        run(
            cmd!(
                "git",
                "-C",
                &repository,
                "config",
                "core.sshCommand",
                SSH_COMMAND,
            ),
            "configure test SSH command",
        );
        let nested = repository.join("nested");
        fs::create_dir(&nested).unwrap();
        let runtime = temp.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        let status = cmd!(
            "sh",
            "-c",
            format!("{SSH_COMMAND} -T -G example.test >/dev/null"),
        )
        .current_dir(&nested)
        .env("TMPDIR", &runtime)
        .status()
        .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_dir(runtime).unwrap().count(), 0);
    }

    #[test]
    fn validates_encrypted_private_key_passphrases() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("identity");
        run(
            cmd!(
                "ssh-keygen",
                "-q",
                "-t",
                "ed25519",
                "-N",
                "secret",
                "-f",
                &key,
            ),
            "generate encrypted key",
        );
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
        let known_hosts = temp.path().join("known_hosts");
        fs::write(&known_hosts, "example.test ssh-ed25519 AAAATEST\n").unwrap();
        let credentials = load_credentials(&key, &known_hosts).unwrap();
        assert!(credentials
            .validate_passphrase(&GitPassphrase::new("wrong".to_owned()))
            .is_err());
        credentials
            .validate_passphrase(&GitPassphrase::new("secret".to_owned()))
            .unwrap();
    }

    #[test]
    fn clone_askpass_executes_outside_noexec_run() {
        let environment = git_environment();
        insta::assert_debug_snapshot!(environment, @r###"
        [
            "GIT_SSH_COMMAND=ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes",
            "SSH_ASKPASS=/tmp/wt-git-askpass",
            "SSH_ASKPASS_REQUIRE=force",
            "DISPLAY=wt:0",
        ]
        "###);
        assert!(!CLONE_ASKPASS.starts_with("/run/"));
    }

    fn run(mut command: Command, action: &str) {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
