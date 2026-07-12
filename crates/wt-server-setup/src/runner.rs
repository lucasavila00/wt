use anyhow::{bail, Context, Result};
use std::process::{Command, Output};

pub(crate) trait Runner {
    fn output(&self, command: Command) -> Result<Output>;

    fn run(&self, command: Command, action: &str) -> Result<()> {
        let output = self.output(command)?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn text(&self, command: Command, action: &str) -> Result<String> {
        let output = self.output(command)?;
        if !output.status.success() {
            bail!(
                "{action}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        String::from_utf8(output.stdout).with_context(|| format!("decode output from {action}"))
    }
}

pub(crate) struct SystemRunner;

impl Runner for SystemRunner {
    fn output(&self, mut command: Command) -> Result<Output> {
        let program = command.get_program().to_string_lossy().into_owned();
        command.output().with_context(|| format!("run {program}"))
    }

    fn run(&self, mut command: Command, action: &str) -> Result<()> {
        let program = command.get_program().to_string_lossy().into_owned();
        let status = command.status().with_context(|| format!("run {program}"))?;
        if !status.success() {
            bail!("{action}: command exited with {status}");
        }
        Ok(())
    }
}
