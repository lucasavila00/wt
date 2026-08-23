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

type PushResultMessage<'a> = &'a dyn Fn(&[u8], &[u8], bool) -> Result<String>;
type ResponseMessage<'a> = &'a dyn Fn(&[u8]) -> Result<String>;

pub trait DuplexStream: Read + Write + Send + 'static {
    fn try_clone_stream(&self) -> std::io::Result<Self>
    where
        Self: Sized;
    fn shutdown_stream(&self, how: Shutdown) -> std::io::Result<()>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitServeResult {
    pub receive_pack: Option<ReceivePackResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivePackResult {
    pub commands: Vec<u8>,
    pub response: Vec<u8>,
    pub sideband: bool,
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
        host_key_policy: HostKeyPolicy<'a>,
        path: &'a str,
    },
}

pub enum HostKeyPolicy<'a> {
    AcceptAny,
    Pinned(&'a Path),
}

impl GitTarget<'_> {
    fn provider_host(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Ssh { host, .. } => Some(host),
        }
    }
}

fn spawn_git(target: &GitTarget<'_>, service: GitService) -> Result<Child> {
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
            host_key_policy,
            path,
        } => {
            let mut command = Command::new("ssh");
            command.arg("-i").arg(private_key_file).args([
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
            ]);
            configure_host_key_policy(&mut command, host_key_policy);
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

fn configure_host_key_policy(command: &mut Command, policy: &HostKeyPolicy<'_>) {
    match policy {
        HostKeyPolicy::Pinned(path) => {
            command
                .arg("-o")
                .arg(format!("UserKnownHostsFile={}", path.display()))
                .args([
                    "-o",
                    "GlobalKnownHostsFile=/dev/null",
                    "-o",
                    "StrictHostKeyChecking=yes",
                ]);
        }
        HostKeyPolicy::AcceptAny => {
            command.args([
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "LogLevel=ERROR",
            ]);
        }
    }
}

pub fn repository_refs(target: GitTarget<'_>) -> Result<Vec<(String, String)>> {
    let mut child = spawn_git(&target, GitService::UploadPack)?;
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
        return Err(error).context(provider_error_context(target.provider_host(), detail));
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
    push_message: Option<PushResultMessage<'_>>,
) -> Result<GitServeResult> {
    let mut child = spawn_git(&target, service)?;
    let provider_host = target.provider_host();
    forward_advertisement(&mut child, stream, provider_host)?;
    if service == GitService::UploadPack {
        bridge_child(stream, child, None, provider_host)?;
        return Ok(GitServeResult::default());
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
        return Ok(GitServeResult::default());
    }
    child
        .stdin
        .as_mut()
        .context("Git service has no stdin")?
        .write_all(&commands)
        .context("forward push commands")?;
    let sideband = push_uses_sideband(&commands)?;
    let message = |response: &[u8]| {
        push_message.context("push message callback is unavailable")?(&commands, response, sideband)
    };
    let response = bridge_child(
        stream,
        child,
        (sideband && push_message.is_some())
            .then_some(&message as &dyn Fn(&[u8]) -> Result<String>),
        provider_host,
    )?
    .expect("receive-pack response is captured");
    Ok(GitServeResult {
        receive_pack: Some(ReceivePackResult {
            commands,
            response,
            sideband,
        }),
    })
}

fn provider_error_context(provider_host: Option<&str>, detail: &str) -> String {
    host_key_verification_error(provider_host, detail)
        .unwrap_or_else(|| format!("Git provider said: {detail}"))
}

fn host_key_verification_error(provider_host: Option<&str>, detail: &str) -> Option<String> {
    let host = provider_host?;
    if !detail.contains("REMOTE HOST IDENTIFICATION HAS CHANGED!")
        && !detail.contains("Host key verification failed.")
    {
        return None;
    }
    Some(format!(
        "Git provider host key verification failed for {host}.\n\
The configured SSH host-key pin does not match the provider.\n\
Update the known-hosts input for the service that launched this Git operation, then reinstall that service.\n\
This cannot be repaired by the downstream Git client."
    ))
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

fn copy_unbuffered(mut from: impl Read, mut to: impl Write) -> std::io::Result<u64> {
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = from.read(&mut buffer)?;
        if count == 0 {
            return Ok(total);
        }
        to.write_all(&buffer[..count])?;
        total += count as u64;
    }
}

fn bridge_child<S: DuplexStream>(
    stream: &mut S,
    mut child: Child,
    push_message: Option<ResponseMessage<'_>>,
    provider_host: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    let stderr = child.stderr.take().context("Git service has no stderr")?;
    let stderr = std::thread::spawn(move || capture_stderr(stderr));
    let mut request_stream = stream.try_clone_stream().context("clone gateway stream")?;
    let shutdown = stream.try_clone_stream().context("clone gateway stream")?;
    let mut child_stdin = child.stdin.take().context("Git service has no stdin")?;
    let request = std::thread::spawn(move || {
        let result = copy_unbuffered(&mut request_stream, &mut child_stdin);
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
        Ok((response.len() as u64, Some(response)))
    } else {
        std::io::copy(&mut child_stdout, stream).map(|count| (count, None))
    };
    let _ = shutdown.shutdown_stream(Shutdown::Both);
    let request = request
        .join()
        .map_err(|_| anyhow::anyhow!("Git request thread panicked"))?;
    tolerate_stream_close(request).context("forward Git request")?;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            tolerate_stream_close(Err(error)).context("forward Git response")?;
            unreachable!("a tolerated stream-close error cannot continue")
        }
    };
    let status = child.wait().context("wait for Git service")?;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("Git stderr reader panicked"))?;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        if !detail.is_empty() {
            if let Some(error) = host_key_verification_error(provider_host, detail) {
                bail!(error);
            }
            bail!("Git provider failed with {status}: {detail}");
        }
        bail!("Git provider failed with {status}");
    }
    Ok(response.1)
}

fn forward_advertisement(
    child: &mut Child,
    stream: &mut impl Write,
    provider_host: Option<&str>,
) -> Result<()> {
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
            return Err(error).context(provider_error_context(provider_host, detail));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_failure_hides_host_credentials_and_addresses_the_operator() {
        let openssh = concat!(
            "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n",
            "@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n",
            "Offending key in /run/credentials/wt-agent-tool-gateway.service/github-ssh-known-hosts:1\n",
            "  remove with: ssh-keygen -f '/run/credentials/wt-agent-tool-gateway.service/github-ssh-known-hosts' -R 'github.com'\n",
            "Host key verification failed."
        );
        let error = anyhow::anyhow!("read Git packet header")
            .context(provider_error_context(Some("github.com"), openssh));
        let rendered = format!("WT Git gateway failed: {error:#}");

        insta::assert_snapshot!(rendered, @r###"
        WT Git gateway failed: Git provider host key verification failed for github.com.
        The configured SSH host-key pin does not match the provider.
        Update the known-hosts input for the service that launched this Git operation, then reinstall that service.
        This cannot be repaired by the downstream Git client.: read Git packet header
        "###);
    }

    #[test]
    fn unpinned_provider_hosts_never_read_or_write_known_hosts() {
        let mut command = Command::new("ssh");
        configure_host_key_policy(&mut command, &HostKeyPolicy::AcceptAny);
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            [
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "LogLevel=ERROR",
            ]
        );
    }
}
