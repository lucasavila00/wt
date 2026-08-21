use std::fs;
use std::io::Write;
use std::path::Path;
use wt_command::cmd;
use wt_libvirt_kvm::WorkerError;

pub(super) fn require_and_read(path: &Path, label: &str) -> Result<Vec<u8>, WorkerError> {
    if !path.is_file() {
        return Err(WorkerError::new(format!(
            "{label} not found: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| context(&format!("read {label}"), error))
}

pub(super) fn verify_registry_cache(url: &str) -> Result<(), WorkerError> {
    let output = cmd!(
        "/usr/bin/curl",
        "-fsS",
        "--output",
        "/dev/null",
        format!("{url}/ca.crt")
    )
    .output()
    .map_err(|error| context("verify registry cache", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "verify registry cache: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

pub(super) fn context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

pub(super) fn log_line(log: &mut dyn Write, message: &str) -> Result<(), WorkerError> {
    writeln!(log, "{message}").map_err(|error| context("write provisioning log", error))
}
