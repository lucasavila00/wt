use crate::config::ClientConfig;
use crate::{inventory, ssh};
use anyhow::{bail, Context as _, Result};
use std::fmt::Write as _;
use std::os::unix::process::CommandExt as _;
use std::process::Command;

pub fn ssh(config: &ClientConfig, target: &str) -> Result<()> {
    let report = inventory::list_all(config);
    if !report.failures.is_empty() {
        let mut message =
            String::from("SSH was not started because the complete world list is unavailable");
        for failure in &report.failures {
            write!(message, "\n\n{}", failure.diagnostic("error").trim_end())?;
        }
        bail!(message);
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

    let qualified = selected.qualified_name();
    Err(Command::new("ssh").args(["--", &qualified]).exec())
        .with_context(|| format!("exec ssh {qualified}"))
}
