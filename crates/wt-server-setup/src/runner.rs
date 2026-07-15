use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Output, Stdio};

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

    fn run_script(&self, script: &[u8], args: &[&str], action: &str) -> Result<()> {
        let mut child = Command::new("/bin/sh")
            .args(["-s", "--"])
            .args(args)
            .stdin(Stdio::piped())
            .spawn()
            .context("start /bin/sh")?;
        child
            .stdin
            .take()
            .context("open /bin/sh stdin")?
            .write_all(script)
            .context("write shell script")?;
        let status = child.wait().context("wait for /bin/sh")?;
        if !status.success() {
            bail!("{action}: script exited with {status}");
        }
        Ok(())
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
