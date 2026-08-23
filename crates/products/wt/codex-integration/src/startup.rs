use crate::{install, reconcile};
use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const LOCK_FILE: &str = "codex-reconciliation.lock";

pub(crate) fn reconcile_manual() -> Result<()> {
    reconcile_before_start(&install::real_codex()?)
}

pub(crate) fn reconcile_before_start(codex: &Path) -> Result<()> {
    let directory = state_directory()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("create state directory {}", directory.display()))?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("secure state directory {}", directory.display()))?;
    let _lock = ProcessLock::acquire(&directory.join(LOCK_FILE))?;
    reconcile::reconcile_with_codex(codex)
}

fn state_directory() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/state/wt"))
}

struct ProcessLock {
    _lock: Flock<File>,
}

impl ProcessLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("open reconciliation lock {}", path.display()))?;
        let lock = Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, error)| error)
            .with_context(|| format!("lock reconciliation state {}", path.display()))?;
        Ok(Self { _lock: lock })
    }
}
