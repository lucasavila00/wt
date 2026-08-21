use anyhow::{bail, Context, Result};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

const REAL_NAME: &str = ".codex.wt-real";
const NEW_NAME: &str = ".codex.wt-new";
const REMOVE_NAME: &str = ".codex.wt-remove";

#[derive(Debug)]
pub(crate) enum InstallOutcome {
    Installed(PathBuf),
    AlreadyInstalled(PathBuf),
}

impl InstallOutcome {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Installed(path) => format!("Installed Codex trampoline: {}", path.display()),
            Self::AlreadyInstalled(path) => {
                format!("Codex trampoline is already installed: {}", path.display())
            }
        }
    }
}

pub(crate) fn invoked_as_codex(args: &[OsString]) -> Result<bool> {
    let argv0 = args.first().context("missing process name")?;
    Ok(Path::new(argv0).file_name() == Some(OsStr::new("codex")))
}

pub(crate) fn active_installation(args: &[OsString]) -> Result<PathBuf> {
    let argv0 = args.first().context("missing process name")?;
    let shim = command_path(argv0)?;
    let real_codex = sibling(&shim, REAL_NAME)?;
    if !real_codex.exists() {
        bail!(
            "the Codex trampoline at {} has no saved CLI at {}",
            shim.display(),
            real_codex.display()
        );
    }
    Ok(real_codex)
}

pub(crate) fn install() -> Result<InstallOutcome> {
    let wt_codex = env::current_exe().context("find the wt-codex executable")?;
    let codex = find_in_path("codex")?;
    install_at(&codex, &wt_codex)
}

pub(crate) fn uninstall() -> Result<PathBuf> {
    let wt_codex = env::current_exe().context("find the wt-codex executable")?;
    let codex = find_in_path("codex")?;
    uninstall_at(&codex, &wt_codex)
}

pub(crate) fn real_codex_in_path() -> Result<PathBuf> {
    let codex = find_in_path("codex")?;
    let wt_codex = env::current_exe().context("find the wt-codex executable")?;
    if is_symlink_to(&codex, &wt_codex)? {
        let real = sibling(&codex, REAL_NAME)?;
        if !real.exists() {
            bail!("Codex trampoline has no saved CLI: {}", real.display());
        }
        Ok(real)
    } else {
        Ok(codex)
    }
}

fn install_at(codex: &Path, wt_codex: &Path) -> Result<InstallOutcome> {
    let real = sibling(codex, REAL_NAME)?;
    if is_symlink_to(codex, wt_codex)? {
        if real.exists() {
            return Ok(InstallOutcome::AlreadyInstalled(codex.to_path_buf()));
        }
        bail!("Codex trampoline has no saved CLI: {}", real.display());
    }
    if real.exists() {
        bail!(
            "refusing to replace Codex because the saved path already exists: {}",
            real.display()
        );
    }

    let temporary = sibling(codex, NEW_NAME)?;
    if temporary.exists() {
        bail!(
            "stale trampoline install path exists: {}",
            temporary.display()
        );
    }
    fs::rename(codex, &real)
        .with_context(|| format!("save the real Codex CLI as {}", real.display()))?;
    if let Err(error) = symlink(wt_codex, &temporary).and_then(|()| fs::rename(&temporary, codex)) {
        let _ = fs::remove_file(&temporary);
        let _ = fs::rename(&real, codex);
        return Err(error).context("install the Codex trampoline");
    }
    Ok(InstallOutcome::Installed(codex.to_path_buf()))
}

fn uninstall_at(codex: &Path, wt_codex: &Path) -> Result<PathBuf> {
    if !is_symlink_to(codex, wt_codex)? {
        bail!("Codex trampoline is not installed at {}", codex.display());
    }
    let real = sibling(codex, REAL_NAME)?;
    if !real.exists() {
        bail!("Codex trampoline has no saved CLI: {}", real.display());
    }
    let temporary = sibling(codex, REMOVE_NAME)?;
    if temporary.exists() {
        bail!(
            "stale trampoline removal path exists: {}",
            temporary.display()
        );
    }

    fs::rename(codex, &temporary).context("stage the Codex trampoline for removal")?;
    if let Err(error) = fs::rename(&real, codex) {
        let _ = fs::rename(&temporary, codex);
        return Err(error).context("restore the real Codex CLI");
    }
    fs::remove_file(&temporary).context("remove the Codex trampoline")?;
    Ok(codex.to_path_buf())
}

fn command_path(argv0: &OsStr) -> Result<PathBuf> {
    let path = Path::new(argv0);
    if path.components().count() > 1 {
        return if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            env::current_dir()
                .context("read the current directory")
                .map(|cwd| cwd.join(path))
        };
    }
    find_in_path(argv0)
}

fn find_in_path(name: impl AsRef<OsStr>) -> Result<PathBuf> {
    let path = env::var_os("PATH").context("PATH is not set")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(name.as_ref());
        if candidate
            .metadata()
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
        {
            return Ok(candidate);
        }
    }
    bail!("Codex CLI was not found in PATH")
}

fn sibling(path: &Path, name: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    Ok(parent.join(name))
}

fn is_symlink_to(path: &Path, target: &Path) -> Result<bool> {
    if !path
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let actual =
        fs::canonicalize(path).with_context(|| format!("resolve symlink {}", path.display()))?;
    let expected = fs::canonicalize(target)
        .with_context(|| format!("resolve executable {}", target.display()))?;
    Ok(actual == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn executable(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn install_is_idempotent_and_uninstall_restores_codex() {
        let temp = tempdir().unwrap();
        let codex = temp.path().join("codex");
        let wt_codex = temp.path().join("wt-codex");
        executable(&codex, "real codex");
        executable(&wt_codex, "wt codex");

        assert!(matches!(
            install_at(&codex, &wt_codex).unwrap(),
            InstallOutcome::Installed(_)
        ));
        assert_eq!(fs::canonicalize(&codex).unwrap(), wt_codex);
        assert_eq!(
            fs::read_to_string(temp.path().join(REAL_NAME)).unwrap(),
            "real codex"
        );
        assert!(matches!(
            install_at(&codex, &wt_codex).unwrap(),
            InstallOutcome::AlreadyInstalled(_)
        ));

        uninstall_at(&codex, &wt_codex).unwrap();
        assert_eq!(fs::read_to_string(&codex).unwrap(), "real codex");
        assert!(!temp.path().join(REAL_NAME).exists());
    }

    #[test]
    fn uninstall_refuses_a_foreign_codex_command() {
        insta::assert_snapshot!(
            uninstall_at(Path::new("/codex"), Path::new("/wt-codex"))
                .unwrap_err()
                .to_string(),
            @"Codex trampoline is not installed at /codex"
        );
    }
}
