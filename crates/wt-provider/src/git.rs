//! SSH-only Git provisioning and the credential bridge into the devcontainer.

use crate::provisioner::guest;
use crate::{GuestTransport, WorkerError};
use nix::unistd::Uid;
use ssh_key::PrivateKey;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;
use wt_api::GitPassphrase;

const GIT_INSTALL: &[u8] = include_bytes!("../../../assets/install-guest-git.sh");
const STAGE: &str = "/tmp/wt-install-git";

#[derive(Clone)]
pub(super) struct Credentials {
    identity: Vec<u8>,
    private_key: PrivateKey,
    known_hosts: Vec<u8>,
}

pub(super) enum Checkout<'a> {
    Branch(&'a str),
    Ref(&'a str),
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

#[allow(clippy::too_many_arguments)]
pub(super) fn install_workspace(
    transport: &dyn GuestTransport,
    source: &str,
    clone_required: bool,
    checkout: Option<Checkout<'_>>,
    credentials: &Credentials,
    passphrase: &GitPassphrase,
    user_name: &str,
    user_email: &str,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    if clone_required {
        credentials.validate_passphrase(passphrase)?;
        for (suffix, contents) in [
            ("-identity", credentials.identity.as_slice()),
            ("-known-hosts", credentials.known_hosts.as_slice()),
            ("-passphrase", passphrase.expose_secret().as_bytes()),
        ] {
            guest::write(transport, &format!("{STAGE}{suffix}"), contents)?;
        }
    }
    let (checkout, checkout_value) = match checkout {
        None => ("none", ""),
        Some(Checkout::Branch(value)) => ("branch", value),
        Some(Checkout::Ref(value)) => ("ref", value),
    };
    let clone = if clone_required { "true" } else { "false" };
    let result = guest::run_script(
        transport,
        "Git workspace installation",
        GIT_INSTALL,
        &[
            source,
            clone,
            checkout,
            checkout_value,
            user_name,
            user_email,
        ],
        deadline,
        log,
    );
    let _ = guest::exec(
        transport,
        "/bin/rm",
        &[
            "-rf",
            "/run/wt-git",
            "/tmp/wt-git-askpass",
            "/tmp/wt-install-git-identity",
            "/tmp/wt-install-git-known-hosts",
            "/tmp/wt-install-git-passphrase",
        ],
        deadline,
    );
    result
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

    fn run(mut command: Command, action: &str) {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
