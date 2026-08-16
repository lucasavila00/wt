mod service;

use crate::{
    api, ClientOperation, ControlRequest, ControlResponse, DuplexStream, GitService, Grant,
    TransportRequest, TransportResponse, BRANCH_PREFIX, PROTOCOL_VERSION,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub state_file: PathBuf,
    pub providers: Vec<Provider>,
}

#[derive(Clone, Debug)]
pub enum Provider {
    Ssh {
        kind: ProviderKind,
        host: String,
        user: String,
        port: Option<u16>,
        api_token_file: PathBuf,
        private_key_file: PathBuf,
        known_hosts_file: PathBuf,
    },
    Local {
        host: String,
        repositories: PathBuf,
        api: Option<FixtureApi>,
    },
}

#[derive(Clone, Debug)]
pub struct FixtureApi {
    pub kind: ProviderKind,
    pub base_url: String,
    pub token_file: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
}

impl Provider {
    fn host(&self) -> &str {
        match self {
            Self::Ssh { host, .. } | Self::Local { host, .. } => host,
        }
    }
}

#[derive(Clone)]
pub struct Gateway {
    config: GatewayConfig,
    state: Arc<Mutex<State>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    grants: Vec<GrantRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GrantRecord {
    id: String,
    token: String,
    world_id: String,
    source: String,
    base: String,
    prefix: String,
    revoked: bool,
}

fn cli_unavailable() -> String {
    "ag-git: provider API commands are not available for this project.\nNormal Git fetch, pull, and push are available.\n".to_owned()
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn git_context_header(grant: &GrantRecord) -> String {
    let project = parse_source(&grant.source)
        .map(|source| source.path.trim_end_matches(".git").to_owned())
        .unwrap_or_else(|_| grant.source.clone());
    format!(
        "remote: This is a WT-managed development environment for a coding agent.\n\
remote: The developer's SSH keys and GitHub or GitLab credentials are not available here.\n\
remote: Do not look for credentials or use gh or glab.\n\
remote: WT gives you scoped access to project {project}.\n\
remote: Use normal Git for commits, fetches, pulls, and pushes.\n\
remote: Every WT world for this project can write branches under {}.\n\
remote: Pull or merge requests target {}.\n\
remote: ag-git uses explicit provider resource types and IDs; it does not infer\n\
remote: resources from the current checkout.\n\
remote: Run ag-git --help to discover every available command.\n\
remote:\n",
        grant.prefix, grant.base
    )
}

struct GitSource {
    host: String,
    user: String,
    port: Option<u16>,
    path: String,
}

fn parse_source(value: &str) -> Result<GitSource> {
    let (user, host_port, path) = if let Some(rest) = value.strip_prefix("ssh://") {
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        let (user, host_port) = authority
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("SSH Git source must include a user"))?;
        (user, host_port, path)
    } else {
        let (authority, path) = value
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        let (user, host) = authority
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        (user, host, path)
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => (
            host,
            Some(port.parse::<u16>().context("invalid SSH Git port")?),
        ),
        _ => (host_port, None),
    };
    if user.is_empty()
        || !valid_host(host)
        || path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        bail!("invalid SSH Git source");
    }
    Ok(GitSource {
        host: host.to_owned(),
        user: user.to_owned(),
        port,
        path: path.to_owned(),
    })
}

fn spawn_git(provider: &Provider, source: &GitSource, service: GitService) -> Result<Child> {
    let mut command = match provider {
        Provider::Local { repositories, .. } => {
            let mut command = Command::new(service.command());
            command.arg(repositories.join(&source.path));
            command
        }
        Provider::Ssh {
            user,
            port,
            private_key_file,
            known_hosts_file,
            ..
        } => {
            if user != &source.user || port != &source.port {
                bail!("Git source does not match the configured SSH endpoint");
            }
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
                .arg(format!("{}@{}", source.user, source.host))
                .arg(service.command())
                .arg(&source.path);
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

fn repository_refs(provider: &Provider, source: &GitSource) -> Result<Vec<(String, String)>> {
    let mut child = spawn_git(provider, source, GitService::UploadPack)?;
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
            let object = fields.next()?;
            let reference = fields.next()?;
            Some((
                String::from_utf8_lossy(object).into_owned(),
                String::from_utf8_lossy(reference).into_owned(),
            ))
        })
        .collect();
    Ok(refs)
}

fn verify_repository(provider: &Provider, source: &GitSource, base: &str) -> Result<()> {
    let refs = repository_refs(provider, source).with_context(|| {
        format!(
            "the gateway SSH key cannot read repository {} on {}",
            source.path, source.host
        )
    })?;
    let expected = format!("refs/heads/{base}");
    if !refs.iter().any(|(_, reference)| reference == &expected) {
        bail!("Git base branch `{base}` does not exist");
    }
    if let Provider::Ssh {
        kind,
        api_token_file,
        ..
    } = provider
    {
        api::verify_provider_access(
            *kind,
            api_token_file,
            &source.host,
            source.path.trim_end_matches(".git"),
            base,
        )
        .context("verify provider API access")?;
    }
    Ok(())
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

fn packet_lines(section: &[u8]) -> Result<impl Iterator<Item = &[u8]>> {
    let mut offset = 0;
    let mut lines = Vec::new();
    while offset + 4 <= section.len() {
        let length = usize::from_str_radix(
            std::str::from_utf8(&section[offset..offset + 4]).context("decode Git packet")?,
            16,
        )
        .context("parse Git packet")?;
        offset += 4;
        if length == 0 {
            break;
        }
        if length < 4 || offset + length - 4 > section.len() {
            bail!("invalid Git packet section");
        }
        lines.push(&section[offset..offset + length - 4]);
        offset += length - 4;
    }
    Ok(lines.into_iter())
}

type PushMessage<'a> = &'a dyn Fn(&[u8]) -> Result<String>;

fn bridge_child<S: DuplexStream>(
    stream: &mut S,
    mut child: Child,
    push_message: Option<PushMessage<'_>>,
) -> Result<()> {
    let stderr = child.stderr.take().context("Git service has no stderr")?;
    let stderr = std::thread::spawn(move || capture_stderr(stderr));
    let mut request_stream = stream.try_clone_stream().context("clone gateway stream")?;
    let shutdown = stream.try_clone_stream().context("clone gateway stream")?;
    let mut child_stdin = child.stdin.take().context("Git service has no stdin")?;
    let request = std::thread::spawn(move || {
        let mut total = 0;
        let mut buffer = [0_u8; 64 * 1024];
        let result = loop {
            match request_stream.read(&mut buffer) {
                Ok(0) => break Ok(total),
                Ok(count) => {
                    if let Err(error) = child_stdin.write_all(&buffer[..count]) {
                        break Err(error);
                    }
                    total += count as u64;
                }
                Err(error) => break Err(error),
            }
        };
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

fn copy_packet_section(mut from: impl Read, mut to: impl Write) -> Result<()> {
    loop {
        let packet = read_packet(&mut from)?;
        to.write_all(&packet).context("forward Git packet")?;
        if packet == b"0000" {
            to.flush().context("flush Git packet section")?;
            return Ok(());
        }
    }
}

fn read_packet_section(mut from: impl Read) -> Result<Vec<u8>> {
    let mut section = Vec::new();
    loop {
        let packet = read_packet(&mut from)?;
        let done = packet == b"0000";
        section.extend_from_slice(&packet);
        if done {
            return Ok(section);
        }
    }
}

fn read_packet(from: &mut impl Read) -> Result<Vec<u8>> {
    let mut header = [0_u8; 4];
    from.read_exact(&mut header)
        .context("read Git packet header")?;
    let header_text = std::str::from_utf8(&header).context("decode Git packet header")?;
    let length = usize::from_str_radix(header_text, 16).context("parse Git packet length")?;
    if length == 0 || length == 1 || length == 2 {
        return Ok(header.to_vec());
    }
    if !(4..=65520).contains(&length) {
        bail!("invalid Git packet length {length}");
    }
    let mut packet = vec![0; length];
    packet[..4].copy_from_slice(&header);
    from.read_exact(&mut packet[4..])
        .context("read Git packet payload")?;
    Ok(packet)
}

fn validate_push(section: &[u8], prefix: &str) -> Result<()> {
    for (_, reference) in push_commands(section)? {
        let Some(branch) = reference.strip_prefix("refs/heads/") else {
            bail!("tags and non-branch refs cannot be pushed from this environment");
        };
        if !branch.starts_with(prefix) || branch.len() == prefix.len() {
            bail!(
                "branch `{branch}` must use the shared `{prefix}` prefix; rename it with `git branch -m {prefix}NAME`"
            );
        }
    }
    Ok(())
}

fn push_commands(section: &[u8]) -> Result<Vec<(String, String)>> {
    let mut offset = 0;
    let mut commands = Vec::new();
    while offset < section.len() {
        if section.len() - offset < 4 {
            bail!("truncated Git push command");
        }
        let length = usize::from_str_radix(
            std::str::from_utf8(&section[offset..offset + 4]).context("decode push packet")?,
            16,
        )
        .context("parse push packet")?;
        offset += 4;
        if length == 0 {
            break;
        }
        if length < 4 || offset + length - 4 > section.len() {
            bail!("invalid Git push command packet");
        }
        let payload = &section[offset..offset + length - 4];
        offset += length - 4;
        let payload = payload.split(|byte| *byte == 0).next().unwrap_or(payload);
        let line = std::str::from_utf8(payload).context("decode Git push command")?;
        let mut fields = line.trim_end_matches('\n').split_whitespace();
        let old = fields.next().context("push command has no old object")?;
        let new = fields.next().context("push command has no new object")?;
        let reference = fields.next().context("push command has no ref")?;
        if fields.next().is_some() || !valid_object_id(old) || !valid_object_id(new) {
            bail!("invalid Git push command");
        }
        commands.push((new.to_owned(), reference.to_owned()));
    }
    if commands.is_empty() {
        bail!("Git push did not contain a ref update");
    }
    Ok(commands)
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn successful_push_updates(
    commands: &[u8],
    response: &[u8],
    sideband: bool,
) -> Result<Vec<(String, String)>> {
    let report = if sideband {
        let mut report = Vec::new();
        for packet in packet_lines(response)? {
            if packet.first() == Some(&1) {
                report.extend_from_slice(&packet[1..]);
            }
        }
        report
    } else {
        response.to_vec()
    };
    let accepted: std::collections::BTreeSet<_> = packet_lines(&report)?
        .filter_map(|line| {
            std::str::from_utf8(line)
                .ok()?
                .trim_end()
                .strip_prefix("ok ")
                .map(str::to_owned)
        })
        .collect();
    push_commands(commands)?
        .into_iter()
        .filter(|(_, reference)| accepted.contains(reference))
        .map(|(head, reference)| {
            let branch = reference
                .strip_prefix("refs/heads/")
                .context("validated push contains a non-branch ref")?;
            Ok((head, branch.to_owned()))
        })
        .collect()
}

fn reject_push(stream: &mut impl Write, section: &[u8], reason: &str) -> Result<()> {
    let mut report = Vec::new();
    write_packet(&mut report, b"unpack ok\n")?;
    for (_, reference) in push_commands(section)? {
        write_packet(&mut report, format!("ng {reference} {reason}\n").as_bytes())?;
    }
    report.extend_from_slice(b"0000");
    if push_uses_sideband(section)? {
        let mut sideband = Vec::with_capacity(report.len() + 1);
        sideband.push(1);
        sideband.extend_from_slice(&report);
        write_packet(stream, &sideband)?;
        stream.write_all(b"0000").context("write sideband end")?;
    } else {
        stream.write_all(&report).context("write push rejection")?;
    }
    stream.flush().context("flush push rejection")
}

fn push_uses_sideband(section: &[u8]) -> Result<bool> {
    if section.len() < 4 {
        bail!("invalid Git push command section");
    }
    let length = usize::from_str_radix(
        std::str::from_utf8(&section[..4]).context("decode push packet")?,
        16,
    )
    .context("parse push packet")?;
    if length < 4 || length > section.len() {
        bail!("invalid Git push command packet");
    }
    let payload = &section[4..length];
    let capabilities = payload
        .iter()
        .position(|byte| *byte == 0)
        .map(|position| &payload[position + 1..])
        .unwrap_or_default();
    Ok(capabilities
        .split(|byte| byte.is_ascii_whitespace())
        .any(|capability| capability == b"side-band-64k" || capability == b"side-band"))
}

fn write_packet(to: &mut impl Write, payload: &[u8]) -> Result<()> {
    let length = payload.len() + 4;
    write!(to, "{length:04x}").context("write Git packet length")?;
    to.write_all(payload).context("write Git packet payload")
}

const HELP: &str = "\
ag-git reads and changes explicitly identified Git provider resources. It accepts\n\
exactly one JSON command object and rejects unknown fields.\n\
\n\
USAGE:\n\
    ag-git '<JSON>'\n\
\n\
TYPESCRIPT COMMAND TYPE:\n\
    type AgGitCommand =\n\
      | { action: \"show_mr\"; mr: number }\n\
      | { action: \"show_mr_for_branch\"; branch: string }\n\
      | { action: \"show_run\"; run: number }\n\
      | { action: \"show_job\"; job: number }\n\
      | { action: \"list_threads\"; mr: number }\n\
      | { action: \"list_ci\"; commit: string }\n\
      | { action: \"list_jobs\"; run: number }\n\
      | { action: \"log_job\"; job: number }\n\
      | { action: \"wait_mr\"; mr: number }\n\
      | { action: \"wait_run\"; run: number }\n\
      | { action: \"wait_job\"; job: number }\n\
      | { action: \"open_mr\"; head: string; base: string; draft?: boolean }\n\
      | { action: \"set_mr\"; mr: number; state: \"ready\" | \"draft\" | \"open\" | \"closed\" }\n\
      | { action: \"edit_mr\"; mr: number; title?: string; body?: string }\n\
      | { action: \"comment_mr\"; mr: number; body: string }\n\
      | { action: \"reply_thread\"; mr: number; thread: string; body: string }\n\
      | { action: \"set_thread\"; mr: number; thread: string; resolved: boolean }\n\
      | { action: \"retry_job\"; job: number }\n\
      | { action: \"cancel_job\"; job: number }\n\
      | { action: \"cancel_run\"; run: number };\n\
\n\
EXAMPLE:\n\
    ag-git '{\"action\":\"show_mr_for_branch\",\"branch\":\"wt/fix-login\"}'\n\
\n\
`show_mr_for_branch` returns the single open MR from the named branch to the\n\
gateway grant's base branch. It fails when there is no match or multiple matches.\n\
\n\
The provider and project come from this world's gateway grant. Every other\n\
resource is explicit. IDs must be positive integers. Commit values must be 7 to\n\
64 hexadecimal characters. Use normal Git for commits, fetches, pulls, and pushes.\n";

#[cfg(test)]
mod tests;
