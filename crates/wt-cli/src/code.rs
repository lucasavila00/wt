use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, serde::Deserialize)]
struct AppInfo {
    workspace: String,
}

pub fn launch(target: &str) -> Result<()> {
    let host = format!("{target}-host");
    let output = Command::new("ssh")
        .args(["--", &host, "/usr/local/bin/wt-app-info"])
        .output()
        .with_context(|| format!("start OpenSSH to inspect {target}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            bail!("inspect {target}: ssh exited with {}", output.status);
        }
        bail!(
            "inspect {target}: ssh exited with {}: {detail}",
            output.status
        );
    }
    let app: AppInfo = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decode app information for {target}"))?;
    if !Path::new(&app.workspace).is_absolute() {
        bail!("app workspace for {target} is not an absolute path");
    }
    let authority = format!("ssh-remote+{target}-vs");
    let status = Command::new("code")
        .args(["--remote", &authority, &app.workspace])
        .status()
        .context("start the VS Code command-line interface (`code`)")?;
    if !status.success() {
        bail!("VS Code exited with {status}");
    }
    Ok(())
}
