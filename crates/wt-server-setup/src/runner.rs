use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::process::{Command, Output};

pub(crate) trait Runner {
    fn output(&self, program: &str, args: &[OsString]) -> Result<Output>;

    fn run(&self, program: &str, args: &[OsString], action: &str) -> Result<()> {
        let output = self.output(program, args)?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn text(&self, program: &str, args: &[OsString], action: &str) -> Result<String> {
        let output = self.output(program, args)?;
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
    fn output(&self, program: &str, args: &[OsString]) -> Result<Output> {
        Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("run {program}"))
    }

    fn run(&self, program: &str, args: &[OsString], action: &str) -> Result<()> {
        let status = Command::new(program)
            .args(args)
            .status()
            .with_context(|| format!("run {program}"))?;
        if !status.success() {
            bail!("{action}: command exited with {status}");
        }
        Ok(())
    }
}

pub(crate) fn args<const N: usize>(values: [&str; N]) -> Vec<OsString> {
    values.into_iter().map(OsString::from).collect()
}
