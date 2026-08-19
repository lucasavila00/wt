use crate::ProxyConfig;
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::path::Path;
use wt_git_core::{serve_git, DuplexStream, GitTarget, WritePolicy};

pub fn serve(config_path: &Path) -> Result<()> {
    let config = ProxyConfig::load(config_path)?;
    let command = std::env::var("SSH_ORIGINAL_COMMAND")
        .context("SSH_ORIGINAL_COMMAND is missing; this command must run through OpenSSH")?;
    let (service, repository, upstream) = config.resolve_command(&command)?;
    let policy = WritePolicy::new(config.write_prefix.clone(), config.allowed_branches.clone())?;
    let mut stream = StdioStream::open()?;
    serve_git(
        &mut stream,
        GitTarget::Ssh {
            host: &upstream.host,
            user: &upstream.user,
            port: upstream.port,
            private_key_file: &upstream.private_key_file,
            known_hosts_file: &upstream.known_hosts_file,
            path: &repository.upstream_path,
        },
        service,
        Some(&policy),
        None,
        None,
    )
}

struct StdioStream {
    input: File,
    output: File,
}

impl StdioStream {
    fn open() -> Result<Self> {
        Ok(Self {
            input: File::open("/dev/stdin").context("open standard input")?,
            output: OpenOptions::new()
                .write(true)
                .open("/dev/stdout")
                .context("open standard output")?,
        })
    }
}

impl Read for StdioStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(buffer)
    }
}

impl Write for StdioStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.output.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

impl DuplexStream for StdioStream {
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        Ok(Self {
            input: self.input.try_clone()?,
            output: self.output.try_clone()?,
        })
    }

    fn shutdown_stream(&self, _how: Shutdown) -> std::io::Result<()> {
        Ok(())
    }
}
