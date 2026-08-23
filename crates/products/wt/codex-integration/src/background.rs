use crate::{install, reconcile};
use anyhow::{bail, Context, Result};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const DESIRED_FILE: &str = "codex-reconciliation-desired";
const STATUS_FILE: &str = "codex-reconciliation-status.json";
const LOCK_FILE: &str = "codex-reconciliation.lock";
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Status {
    Reconciling {
        generation: String,
        codex_version: String,
    },
    Ready {
        generation: String,
        codex_version: String,
    },
    Failed {
        generation: String,
        codex_version: String,
        error: String,
    },
}

pub(crate) fn require_ready(codex: &Path) -> Result<()> {
    require_ready_at(&state_directory()?, &codex_version(codex)?)
}

pub(crate) fn reconcile_manual() -> Result<()> {
    let directory = state_directory()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("create state directory {}", directory.display()))?;
    let _lock = ProcessLock::acquire(&directory.join(LOCK_FILE), false)?
        .context("Codex session reconciliation is already running")?;
    reconcile::reconcile()
}

pub(crate) fn reconcile_worker() -> Result<()> {
    reconcile_worker_at(&state_directory()?)
}

fn reconcile_worker_at(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("create state directory {}", directory.display()))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("secure state directory {}", directory.display()))?;
    let Some(_lock) = ProcessLock::acquire(&directory.join(LOCK_FILE), true)? else {
        return Ok(());
    };
    let codex = install::real_codex()?;
    let version = codex_version(&codex)?;

    loop {
        let generation = read_generation(&directory.join(DESIRED_FILE))?;
        if should_publish_reconciling(directory, &generation)? {
            write_status(
                directory,
                &Status::Reconciling {
                    generation: generation.clone(),
                    codex_version: version.clone(),
                },
            )?;
        }
        if let Err(error) = reconcile::reconcile_with_codex(&codex) {
            let diagnostic = bounded(&format!("{error:#}"));
            write_status(
                directory,
                &Status::Failed {
                    generation,
                    codex_version: version,
                    error: diagnostic.clone(),
                },
            )?;
            bail!(diagnostic);
        }
        write_status(
            directory,
            &Status::Ready {
                generation: generation.clone(),
                codex_version: version.clone(),
            },
        )?;
        if read_generation(&directory.join(DESIRED_FILE))? == generation {
            return Ok(());
        }
    }
}

fn should_publish_reconciling(directory: &Path, generation: &str) -> Result<bool> {
    let path = directory.join(STATUS_FILE);
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("read reconciliation status {}", path.display()))
        }
    };
    let status: Status = serde_json::from_slice(&contents)
        .with_context(|| format!("parse reconciliation status {}", path.display()))?;
    Ok(!matches!(
        status,
        Status::Failed {
            generation: failed_generation,
            ..
        } if failed_generation == generation
    ))
}

fn require_ready_at(directory: &Path, codex_version: &str) -> Result<()> {
    let desired_path = directory.join(DESIRED_FILE);
    let desired = match read_generation(&desired_path) {
        Ok(generation) => generation,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            bail!("Codex history has not been prepared in this world; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history")
        }
        Err(error) => return Err(error),
    };
    let status_path = directory.join(STATUS_FILE);
    let status: Status = match fs::read(&status_path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .with_context(|| format!("parse reconciliation status {}", status_path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("Codex history preparation is pending; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history")
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("read reconciliation status {}", status_path.display()));
        }
    };
    match status {
        Status::Ready {
            generation,
            codex_version: applied_version,
        } if generation == desired && applied_version == codex_version => Ok(()),
        Status::Reconciling { generation, .. } if generation == desired => bail!(
            "Codex history reconciliation is running; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history"
        ),
        Status::Failed {
            generation, error, ..
        } if generation == desired => bail!(
            "Codex history reconciliation failed: {error}; status: {}; set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history",
            status_path.display()
        ),
        _ => bail!(
            "Codex history changed and reconciliation is pending; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history"
        ),
    }
}

fn read_generation(path: &Path) -> Result<String> {
    let generation = fs::read_to_string(path)
        .with_context(|| format!("read desired reconciliation generation {}", path.display()))?;
    let generation = generation.trim();
    if generation.len() != 64
        || !generation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("desired Codex reconciliation generation is invalid");
    }
    Ok(generation.to_owned())
}

fn codex_version(codex: &Path) -> Result<String> {
    let output = Command::new(codex)
        .arg("--version")
        .output()
        .with_context(|| format!("read {} version", codex.display()))?;
    if !output.status.success() {
        bail!(
            "read Codex version: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let version = String::from_utf8(output.stdout).context("Codex version is not UTF-8")?;
    let version = version.trim();
    if version.is_empty() {
        bail!("Codex version is empty");
    }
    Ok(version.to_owned())
}

fn write_status(directory: &Path, status: &Status) -> Result<()> {
    let path = directory.join(STATUS_FILE);
    let temporary = directory.join(format!(".{STATUS_FILE}.{}", std::process::id()));
    let mut contents = serde_json::to_vec(status).context("encode reconciliation status")?;
    contents.push(b'\n');
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("create reconciliation status {}", temporary.display()))?;
        file.write_all(&contents)
            .with_context(|| format!("write reconciliation status {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync reconciliation status {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("publish reconciliation status {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn bounded(value: &str) -> String {
    let mut end = value.len().min(MAX_DIAGNOSTIC_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn state_directory() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/state/wt"))
}

struct ProcessLock {
    _lock: Flock<File>,
}

impl ProcessLock {
    fn acquire(path: &Path, nonblocking: bool) -> Result<Option<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("open reconciliation lock {}", path.display()))?;
        let argument = if nonblocking {
            FlockArg::LockExclusiveNonblock
        } else {
            FlockArg::LockExclusive
        };
        match Flock::lock(file, argument) {
            Ok(lock) => Ok(Some(Self { _lock: lock })),
            Err((_, nix::errno::Errno::EWOULDBLOCK)) if nonblocking => Ok(None),
            Err((_, error)) => {
                Err(error).with_context(|| format!("lock reconciliation state {}", path.display()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const SECOND: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    fn write_desired(directory: &Path, generation: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join(DESIRED_FILE), format!("{generation}\n")).unwrap();
    }

    #[test]
    fn readiness_requires_the_desired_generation() {
        let temp = tempfile::tempdir().unwrap();
        write_desired(temp.path(), FIRST);
        write_status(
            temp.path(),
            &Status::Ready {
                generation: FIRST.into(),
                codex_version: "codex-cli 0.149.0".into(),
            },
        )
        .unwrap();
        assert!(require_ready_at(temp.path(), "codex-cli 0.149.0").is_ok());

        write_desired(temp.path(), SECOND);
        insta::assert_snapshot!(
            require_ready_at(temp.path(), "codex-cli 0.149.0").unwrap_err().to_string(),
            @"Codex history changed and reconciliation is pending; retry shortly or set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history"
        );
    }

    #[test]
    fn readiness_reports_background_failure() {
        let temp = tempfile::tempdir().unwrap();
        write_desired(temp.path(), FIRST);
        write_status(
            temp.path(),
            &Status::Failed {
                generation: FIRST.into(),
                codex_version: "codex-cli 0.149.0".into(),
                error: "database refused migration".into(),
            },
        )
        .unwrap();

        let error = require_ready_at(temp.path(), "codex-cli 0.149.0")
            .unwrap_err()
            .to_string()
            .replace(&temp.path().display().to_string(), "<STATE>");
        insta::assert_snapshot!(error, @"Codex history reconciliation failed: database refused migration; status: <STATE>/codex-reconciliation-status.json; set IGNORE_CODEX_WT_CHECKS=true to start without synchronized history");
    }

    #[test]
    fn keeps_a_failure_visible_while_retrying_the_same_generation() {
        let temp = tempfile::tempdir().unwrap();
        write_desired(temp.path(), FIRST);
        write_status(
            temp.path(),
            &Status::Failed {
                generation: FIRST.into(),
                codex_version: "codex-cli 0.149.0".into(),
                error: "Codex app-server did not reply".into(),
            },
        )
        .unwrap();

        assert!(!should_publish_reconciling(temp.path(), FIRST).unwrap());
        assert!(should_publish_reconciling(temp.path(), SECOND).unwrap());
    }
}
