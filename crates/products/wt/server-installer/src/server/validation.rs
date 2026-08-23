use super::{load_install_input, validate_agent_tools_files};
use anyhow::{bail, Context, Result};
use nix::unistd::{chown, Gid};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const E2E_FIXTURE_DIRECTORY: &str = "/home/wt/.config/wt/kvm-test";

pub(crate) fn validate(input_path: &Path) -> Result<()> {
    let (input, _, _) = load_install_input(input_path)?;
    validate_agent_tools_files(&input)
}

pub(crate) fn validate_e2e(input_path: &Path) -> Result<()> {
    let (input, _, _) = load_install_input(input_path)?;
    if !input.test_server {
        bail!(
            "refusing destructive E2E setup: {} has test_server = false",
            input_path.display()
        );
    }
    validate_agent_tools_files(&input)
}

pub(crate) fn prepare_e2e(input_path: &Path) -> Result<()> {
    let input =
        crate::install_input::InstallInput::load_from(input_path).map_err(anyhow::Error::msg)?;
    if !input.test_server {
        bail!(
            "refusing E2E fixture preparation: {} has test_server = false",
            input_path.display()
        );
    }
    prepare_test_provider_fixtures(&input)?;
    let paths = input.materialize().codex_paths();
    prepare_test_codex_fixture(paths.auth, paths.auth_share, paths.sessions)?;
    validate_agent_tools_files(&input)
}

fn prepare_test_provider_fixtures(input: &crate::install_input::InstallInput) -> Result<()> {
    for (_, provider) in input.agent_tools.providers() {
        for path in [
            &provider.api_token_file,
            &provider.ssh_private_key_file,
            &provider.ssh_public_key_file,
        ] {
            require_test_fixture_path(path)?;
            let parent = path.parent().ok_or_else(|| {
                anyhow::anyhow!("test fixture path has no parent: {}", path.display())
            })?;
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            set_test_fixture_group(parent)?;
        }
        if !provider.api_token_file.exists() {
            let mut token = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&provider.api_token_file)?;
            token.write_all(b"not-a-real-token\n")?;
            token.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        if !provider.ssh_private_key_file.exists() {
            let status = Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&provider.ssh_private_key_file)
                .status()
                .context("create disposable E2E SSH key")?;
            if !status.success() {
                bail!("create disposable E2E SSH key failed: {status}");
            }
        }
        if !provider.ssh_public_key_file.exists() {
            bail!(
                "missing public key for disposable E2E SSH key: {}",
                provider.ssh_public_key_file.display()
            );
        }
        for path in [
            &provider.api_token_file,
            &provider.ssh_private_key_file,
            &provider.ssh_public_key_file,
        ] {
            set_test_fixture_group(path)?;
        }
    }
    Ok(())
}

fn require_test_fixture_path(path: &Path) -> Result<()> {
    if path.starts_with(E2E_FIXTURE_DIRECTORY) {
        Ok(())
    } else {
        bail!(
            "E2E fixture path must be below {E2E_FIXTURE_DIRECTORY}: {}",
            path.display()
        )
    }
}

fn set_test_fixture_group(path: &Path) -> Result<()> {
    chown(
        path,
        None,
        Some(Gid::from_raw(wt_retained_worlds::GUEST_GID)),
    )
    .with_context(|| format!("set WT group on E2E fixture {}", path.display()))
}

fn prepare_test_codex_fixture(auth: &str, auth_share: &str, sessions: &str) -> Result<()> {
    let auth = Path::new(auth);
    require_test_fixture_path(auth)?;
    require_test_fixture_path(Path::new(auth_share))?;
    require_test_fixture_path(Path::new(sessions))?;
    prepare_codex_fixture(auth, Path::new(auth_share), Path::new(sessions))
}

fn prepare_codex_fixture(auth: &Path, auth_share: &Path, sessions: &Path) -> Result<()> {
    let root = auth
        .parent()
        .ok_or_else(|| anyhow::anyhow!("test Codex auth has no parent"))?;
    for directory in [root, auth_share, sessions] {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        set_test_fixture_group(directory)?;
    }
    if !auth.exists() {
        let mut file = OpenOptions::new().write(true).create_new(true).open(auth)?;
        file.write_all(b"{\"test\":true}\n")?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    set_test_fixture_group(auth)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_fixture_is_isolated_and_does_not_overwrite_auth() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("codex");
        let auth = root.join("auth.json");
        let share = root.join("auth-share");
        let sessions = root.join("sessions");

        prepare_codex_fixture(&auth, &share, &sessions).unwrap();
        assert_eq!(fs::read(&auth).unwrap(), b"{\"test\":true}\n");
        assert_eq!(
            fs::metadata(&auth).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&share).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&sessions).unwrap().permissions().mode() & 0o777,
            0o700
        );

        fs::write(&auth, b"existing\n").unwrap();
        prepare_codex_fixture(&auth, &share, &sessions).unwrap();
        assert_eq!(fs::read(&auth).unwrap(), b"existing\n");
    }
}
