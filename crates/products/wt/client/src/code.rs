use anyhow::{bail, Context as _, Result};
use std::process::Command;
use wt_client::config::ClientConfig;
use wt_client::{inventory, ssh};

pub(super) fn open(config: &ClientConfig, target: &str) -> Result<()> {
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        return Err(super::context_failures(
            "VS Code was not opened because the complete world list is unavailable",
            &report.failures,
            None,
        ));
    }

    let selected = inventory::resolve(&report.worlds, target)?;
    if !ssh::has_alias(selected) {
        bail!(
            "world {} has no managed SSH alias in status {}",
            selected.qualified_name(),
            selected.world.status
        );
    }
    ssh::sync(config, &report.worlds)?;

    let authority = format!("ssh-remote+{}-direct", selected.qualified_name());
    let status = Command::new("code")
        .args(["--remote", &authority])
        .status()
        .context("start the VS Code command-line interface (`code`)")?;
    if !status.success() {
        bail!("VS Code exited with {status}");
    }
    Ok(())
}
