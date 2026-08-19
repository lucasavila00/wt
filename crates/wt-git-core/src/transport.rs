use crate::packet::{
    copy_packet_section, packet_lines, push_uses_sideband, read_packet_section, reject_push,
    write_packet,
};
use crate::policy::{push_violation, PushViolation, WritePolicy};
use crate::GitService;
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub trait DuplexStream: Read + Write + Send + 'static {
    fn try_clone_stream(&self) -> std::io::Result<Self>
    where
        Self: Sized;
    fn shutdown_stream(&self, how: Shutdown) -> std::io::Result<()>;
}

impl DuplexStream for UnixStream {
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }

    fn shutdown_stream(&self, how: Shutdown) -> std::io::Result<()> {
        self.shutdown(how)
    }
}

pub enum GitTarget<'a> {
    Local {
        repositories: &'a Path,
        path: &'a str,
    },
    Ssh {
        host: &'a str,
        user: &'a str,
        port: Option<u16>,
        private_key_file: &'a Path,
        known_hosts_file: &'a Path,
        path: &'a str,
    },
}

fn spawn_git(target: GitTarget<'_>, service: GitService) -> Result<Child> {
    let mut command = match target {
        GitTarget::Local { repositories, path } => {
            let mut command = Command::new(service.command());
            command.arg(repositories.join(path));
            command
        }
        GitTarget::Ssh {
            host,
            user,
            port,
            private_key_file,
            known_hosts_file,
            path,
        } => {
            let mut command = Command::new("ssh");
            command
                .arg("-i")
                .arg(private_key_file)
                .args(["-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes"])
                .arg("-o")
                .arg(format!("UserKnownHostsFile={}", known_hosts_file.display()))
                .args(["-o", "StrictHostKeyChecking=yes"]);
            if let Some(port) = port {
                command.args(["-p", &port.to_string()]);
            }
            command
                .arg(format!("{user}@{host}"))
                .arg(service.command())
                .arg(path);
            command
        }
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {}", service.command()))
}

pub fn repository_refs(target: GitTarget<'_>) -> Result<Vec<(String, String)>> {
    let mut child = spawn_git(target, GitService::UploadPack)?;
    let stderr = child.stderr.take().context("Git service has no stderr")?;
    let stderr = std::thread::spawn(move || capture_stderr(stderr));
    let mut advertisement = Vec::new();
    let result = copy_packet_section(
        child.stdout.as_mut().context("Git service has no stdout")?,
        &mut advertisement,
    );
    let _ = child.kill();
    let _ = child.wait();
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("Git stderr reader panicked"))?;
    if let Err(error) = result {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        if detail.is_empty() {
            return Err(error).context("read Git repository advertisement");
        }
        return Err(error).context(format!("Git provider said: {detail}"));
    }
    let refs = packet_lines(&advertisement)?
        .filter_map(|line| {
            let mut fields = line.split(|byte| byte.is_ascii_whitespace() || *byte == 0);
            Some((
                String::from_utf8_lossy(fields.next()?).into_owned(),
                String::from_utf8_lossy(fields.next()?).into_owned(),
            ))
        })
        .collect();
    Ok(refs)
}

pub fn serve_git<S: DuplexStream>(
    stream: &mut S,
    target: GitTarget<'_>,
    service: GitService,
    policy: Option<&WritePolicy>,
    rejection_message: Option<&dyn Fn(&PushViolation) -> String>,
    push_message: Option<&dyn Fn(&[u8], &[u8], bool) -> Result<String>>,
) -> Result<()> {
    let mut child = spawn_git(target, service)?;
    forward_advertisement(&mut child, stream)?;
    if service == GitService::UploadPack {
        return bridge_child(stream, child, None);
    }

    let policy = policy.context("receive-pack needs a write policy")?;
    let commands = read_packet_section(&mut *stream)?;
    if let Some(violation) = push_violation(&commands, policy)? {
        let reason = rejection_message
            .map(|message| message(&violation))
            .unwrap_or_else(|| violation.to_string());
        reject_push(stream, &commands, &reason)?;
        let _ = child.kill();
        let _ = child.wait();
        return Ok(());
    }
    child
        .stdin
        .as_mut()
        .context("Git service has no stdin")?
        .write_all(&commands)
        .context("forward push commands")?;
    let sideband = push_uses_sideband(&commands)?;
    let message = |response: &[u8]| {
        push_message.context("push message callback is unavailable")?(
            &commands, response, sideband,
        )
    };
    bridge_child(
        stream,
        child,
        (sideband && push_message.is_some())
            .then_some(&message as &dyn Fn(&[u8]) -> Result<String>),
    )
}

fn capture_stderr(mut stderr: impl Read) -> Vec<u8> {
    const LIMIT: usize = 16 * 1024;
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return captured,
            Ok(count) if captured.len() < LIMIT => {
                let remaining = LIMIT - captured.len();
                captured.extend_from_slice(&buffer[..count.min(remaining)]);
            }
            Ok(_) => {}
        }
    }
}

fn bridge_child<S: DuplexStream>(
    stream: &mut S,
    mut child: Child,
    push_message: Option<&dyn Fn(&[u8]) -> Result<String>>,
) -> Result<()> {
    let stderr = child.stderr.take().context("Git service has no stderr")?;
    let stderr = std::thread::spawn(move || capture_stderr(stderr));
    let mut request_stream = stream.try_clone_stream().context("clone gateway stream")?;
    let shutdown = stream.try_clone_stream().context("clone gateway stream")?;
    let mut child_stdin = child.stdin.take().context("Git service has no stdin")?;
    let request = std::thread::spawn(move || {
        let result = std::io::copy(&mut request_stream, &mut child_stdin);
        drop(child_stdin);
        result
    });
    let mut child_stdout = child.stdout.take().context("Git service has no stdout")?;
    let response = if let Some(push_message) = push_message {
        let mut response = Vec::new();
        child_stdout
            .read_to_end(&mut response)
            .context("read Git response")?;
        if let Some(body) = response.strip_suffix(b"0000") {
            stream.write_all(body).context("forward Git response")?;
            let message = push_message(&response)?;
            if !message.is_empty() {
                let mut packet = Vec::with_capacity(message.len() + 1);
                packet.push(2);
                packet.extend_from_slice(message.as_bytes());
                write_packet(stream, &packet)?;
            }
            stream.write_all(b"0000").context("finish Git response")?;
        } else {
            stream
                .write_all(&response)
                .context("forward Git response")?;
        }
        stream.flush().context("flush Git response")?;
        Ok(response.len() as u64)
    } else {
        std::io::copy(&mut child_stdout, stream)
    };
    let _ = shutdown.shutdown_stream(Shutdown::Both);
    let request = request
        .join()
        .map_err(|_| anyhow::anyhow!("Git request thread panicked"))?;
    tolerate_stream_close(request).context("forward Git request")?;
    tolerate_stream_close(response).context("forward Git response")?;
    let status = child.wait().context("wait for Git service")?;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("Git stderr reader panicked"))?;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        if !detail.is_empty() {
            bail!("Git provider failed with {status}: {detail}");
        }
        bail!("Git provider failed with {status}");
    }
    Ok(())
}

fn forward_advertisement(child: &mut Child, stream: &mut impl Write) -> Result<()> {
    let result = copy_packet_section(
        child.stdout.as_mut().context("Git service has no stdout")?,
        stream,
    );
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        let stderr = child.stderr.take().map(capture_stderr).unwrap_or_default();
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        if !detail.is_empty() {
            return Err(error).context(format!("Git provider said: {detail}"));
        }
        return Err(error).context("read Git provider response");
    }
    Ok(())
}

fn tolerate_stream_close(result: std::io::Result<u64>) -> std::io::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotConnected
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}
