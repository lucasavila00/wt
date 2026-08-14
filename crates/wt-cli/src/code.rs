use anyhow::{bail, Context as _, Result};
use std::path::Path;
use std::process::Command;
use wt_api::WorldKind;
use wt_cli::config::ClientConfig;
use wt_cli::inventory;

#[derive(Debug, serde::Deserialize)]
struct AppInfo {
    workspace: String,
}

pub(super) fn open(config: &ClientConfig, target: &str) -> Result<()> {
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(super::context_failures(
            "VS Code was not opened because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }
    let selected = inventory::resolve(&report.instances, target)?;
    if selected.instance.status != wt_api::InstanceStatus::Running {
        bail!(
            "world {} is {}; VS Code can only open a running world",
            selected.qualified_name(),
            selected.instance.status
        );
    }
    if selected.instance.kind() != WorldKind::Devcontainer {
        bail!(
            "world {} is {}; VS Code only supports devcontainer worlds",
            selected.qualified_name(),
            selected.instance.kind()
        );
    }
    if selected.instance.ssh.is_none() || selected.instance.application.app_ssh().is_none() {
        bail!(
            "world {} has incomplete SSH access information",
            selected.qualified_name()
        );
    }

    wt_cli::ssh::sync(config, &report.instances)?;
    let qualified = selected.qualified_name();
    let workspace = discover_app_workspace(&qualified)?;
    launch(&qualified, &workspace)
}

fn discover_app_workspace(qualified: &str) -> Result<String> {
    let host = format!("{qualified}-host");
    let output = Command::new("ssh")
        .args(["--", &host, "/usr/local/bin/wt-app-info"])
        .output()
        .with_context(|| format!("start OpenSSH to inspect {qualified}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            bail!("inspect {qualified}: ssh exited with {}", output.status);
        }
        bail!(
            "inspect {qualified}: ssh exited with {}: {detail}",
            output.status
        );
    }
    let app: AppInfo = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decode app information for {qualified}"))?;
    if !Path::new(&app.workspace).is_absolute() {
        bail!("app workspace for {qualified} is not an absolute path");
    }
    Ok(app.workspace)
}

fn launch(qualified: &str, workspace: &str) -> Result<()> {
    let authority = format!("ssh-remote+{qualified}-vs");
    let status = Command::new("code")
        .args(["--remote", &authority, workspace])
        .status()
        .context("start the VS Code command-line interface (`code`)")?;
    if !status.success() {
        bail!("VS Code exited with {status}");
    }
    Ok(())
}
