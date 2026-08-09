use crate::{
    ClientOperation, ControlRequest, ControlResponse, DuplexStream, GitService, Grant,
    TransportRequest, TransportResponse, PROTOCOL_VERSION,
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
        host: String,
        user: String,
        port: Option<u16>,
        private_key_file: PathBuf,
        known_hosts_file: PathBuf,
    },
    Local {
        host: String,
        repositories: PathBuf,
    },
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

impl Gateway {
    pub fn open(config: GatewayConfig) -> Result<Self> {
        if config.providers.is_empty() {
            bail!("at least one Git provider is required");
        }
        let mut hosts = std::collections::BTreeSet::new();
        for provider in &config.providers {
            if !valid_host(provider.host()) || !hosts.insert(provider.host()) {
                bail!(
                    "invalid or duplicate Git provider host: {}",
                    provider.host()
                );
            }
        }
        let state = match fs::read(&config.state_file) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("decode gateway state")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error).context("read gateway state"),
        };
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn handle_control<S: Read + Write>(&self, mut stream: S) -> Result<()> {
        let request: ControlRequest = crate::read_json_line(&mut stream)?;
        let response = match self.control(request) {
            Ok(response) => response,
            Err(error) => ControlResponse::error(format!("{error:#}")),
        };
        crate::write_json_line(&mut stream, &response)
    }

    pub fn control(&self, request: ControlRequest) -> Result<ControlResponse> {
        match request {
            ControlRequest::Reserve {
                world_id,
                source,
                base,
                prefix,
            } => self.reserve(&world_id, &source, &base, &prefix),
            ControlRequest::Revoke { grant_id } => self.revoke(&grant_id),
        }
    }

    pub fn handle_transport<S: DuplexStream>(&self, mut stream: S) -> Result<()> {
        let request: TransportRequest = crate::read_json_line(&mut stream)?;
        let result = self.authorize(&request);
        let grant = match result {
            Ok(grant) => grant,
            Err(error) => {
                crate::write_json_line(
                    &mut stream,
                    &TransportResponse::error(format!("{error:#}")),
                )?;
                return Ok(());
            }
        };
        crate::write_json_line(&mut stream, &TransportResponse::ok())?;
        match request.operation {
            ClientOperation::Git { service, source } => {
                self.serve_git(stream, service, &source, &grant)
            }
            ClientOperation::Cli { args } => self.serve_cli(stream, &args, &grant),
        }
    }

    fn reserve(
        &self,
        world_id: &str,
        source: &str,
        base: &str,
        prefix: &str,
    ) -> Result<ControlResponse> {
        if world_id.is_empty() || base.is_empty() || !valid_prefix(prefix) {
            bail!("invalid gateway grant scope");
        }
        let parsed = parse_source(source)?;
        self.provider(&parsed.host)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
        if let Some(existing) = state.grants.iter().find(|grant| grant.world_id == world_id) {
            if existing.source != source || existing.base != base || existing.prefix != prefix {
                bail!("world already reserved with a different Git scope");
            }
            return Ok(ControlResponse::ok(Some(Grant {
                id: existing.id.clone(),
                token: existing.token.clone(),
            })));
        }
        if state
            .grants
            .iter()
            .any(|grant| !grant.revoked && grant.source == source && grant.prefix == prefix)
        {
            bail!("branch prefix {prefix} is already reserved for this project");
        }
        let record = GrantRecord {
            id: Uuid::new_v4().to_string(),
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            world_id: world_id.to_owned(),
            source: source.to_owned(),
            base: base.to_owned(),
            prefix: prefix.to_owned(),
            revoked: false,
        };
        let response = ControlResponse::ok(Some(Grant {
            id: record.id.clone(),
            token: record.token.clone(),
        }));
        state.grants.push(record);
        self.save(&state)?;
        Ok(response)
    }

    fn revoke(&self, id: &str) -> Result<ControlResponse> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
        let grant = state
            .grants
            .iter_mut()
            .find(|grant| grant.id == id)
            .ok_or_else(|| anyhow::anyhow!("gateway grant not found"))?;
        grant.revoked = true;
        self.save(&state)?;
        Ok(ControlResponse::ok(None))
    }

    fn authorize(&self, request: &TransportRequest) -> Result<GrantRecord> {
        if request.protocol_version != PROTOCOL_VERSION {
            bail!(
                "unsupported gateway protocol version {}; expected {PROTOCOL_VERSION}",
                request.protocol_version
            );
        }
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
        let grant = state
            .grants
            .iter()
            .find(|grant| grant.token == request.token && !grant.revoked)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("gateway grant is invalid or revoked"))?;
        if let ClientOperation::Git { source, .. } = &request.operation {
            if source != &grant.source {
                bail!("gateway grant does not allow project {source}");
            }
        }
        Ok(grant)
    }

    fn save(&self, state: &State) -> Result<()> {
        let parent = self
            .config
            .state_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("gateway state file has no parent"))?;
        fs::create_dir_all(parent).context("create gateway state directory")?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .context("protect gateway state directory")?;
        let temporary = self.config.state_file.with_extension("json.new");
        let bytes = serde_json::to_vec_pretty(state).context("encode gateway state")?;
        fs::write(&temporary, bytes).context("write gateway state")?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .context("protect gateway state")?;
        fs::rename(&temporary, &self.config.state_file).context("replace gateway state")
    }

    fn provider(&self, host: &str) -> Result<&Provider> {
        self.config
            .providers
            .iter()
            .find(|provider| provider.host() == host)
            .ok_or_else(|| anyhow::anyhow!("Git provider {host} is not configured"))
    }

    fn serve_git<S: DuplexStream>(
        &self,
        mut stream: S,
        service: GitService,
        source: &str,
        grant: &GrantRecord,
    ) -> Result<()> {
        let source = parse_source(source)?;
        let provider = self.provider(&source.host)?;
        let mut child = spawn_git(provider, &source, service)?;
        if service == GitService::ReceivePack {
            copy_packet_section(
                child.stdout.as_mut().context("Git service has no stdout")?,
                &mut stream,
            )?;
            let commands = read_packet_section(&mut stream)?;
            if let Err(error) = validate_push(&commands, &grant.prefix) {
                reject_push(&mut stream, &commands, &error.to_string())?;
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
        }
        bridge_child(stream, child)
    }

    fn serve_cli<S: DuplexStream>(
        &self,
        mut stream: S,
        args: &[String],
        grant: &GrantRecord,
    ) -> Result<()> {
        let output = if args == ["--help"] || args == ["-h"] {
            HELP.to_owned()
        } else {
            format!(
                "WT agent Git environment\n\nProject: {}\nBranch prefix: {}\nPull or merge request support is not installed yet.\nRun `ag-git --help` to see the command contract.\n",
                grant.source, grant.prefix
            )
        };
        stream
            .write_all(output.as_bytes())
            .context("write ag-git output")
    }
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn valid_prefix(value: &str) -> bool {
    value.ends_with('/')
        && value.len() > 1
        && value[..value.len() - 1]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
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
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("start {}", service.command()))
}

fn bridge_child<S: DuplexStream>(mut stream: S, mut child: Child) -> Result<()> {
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
    let response = std::io::copy(&mut child_stdout, &mut stream);
    let _ = shutdown.shutdown_stream(Shutdown::Both);
    let request = request
        .join()
        .map_err(|_| anyhow::anyhow!("Git request thread panicked"))?;
    tolerate_stream_close(request).context("forward Git request")?;
    tolerate_stream_close(response).context("forward Git response")?;
    let status = child.wait().context("wait for Git service")?;
    if !status.success() {
        bail!("{} exited with {status}", "Git service");
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
                "branch `{branch}` must use this world's `{prefix}` prefix; rename it with `git branch -m {prefix}NAME`"
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
        commands.push((reference.to_owned(), reference.to_owned()));
    }
    if commands.is_empty() {
        bail!("Git push did not contain a ref update");
    }
    Ok(commands)
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
ag-git manages the pull or merge request for the current WT branch.\n\
\n\
USAGE:\n\
    ag-git [COMMAND] [OPTIONS]\n\
\n\
Run `ag-git` with no command to show the current branch, request, reviews, and CI.\n\
\n\
COMMANDS:\n\
    open-mr [--draft]       Open or show the branch's pull or merge request\n\
    ready                   Mark the request ready for review\n\
    draft                   Return the request to draft\n\
    comment TEXT            Add a request comment\n\
    review                  Show review threads and their handles\n\
    reply HANDLE TEXT       Reply to a review thread\n\
    resolve HANDLE          Resolve a review thread\n\
    reopen HANDLE           Reopen a review thread\n\
    ci                      Show CI jobs for the current commit\n\
    log JOB                 Show one CI job's log\n\
    retry JOB               Retry a CI job when the provider allows it\n\
    cancel JOB              Cancel a CI job when the provider allows it\n\
    wait                    Wait for review or CI state to change\n\
    close                   Close the request\n\
    reopen-mr               Reopen the request\n\
    help                    Show this help\n\
\n\
Use normal Git for commits, fetches, pulls, and pushes.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_sources_without_shell_syntax() {
        let source = parse_source("git@example.test:group/repo.git").unwrap();
        assert_eq!(source.host, "example.test");
        assert_eq!(source.path, "group/repo.git");
        assert!(parse_source("git@example.test:group/repo;touch-pwned").is_err());
        assert!(parse_source("git@example.test:../repo.git").is_err());
    }

    #[test]
    fn push_scope_allows_only_prefixed_heads() {
        let command = |reference: &str| {
            let payload = format!(
                "{} {} {}\0report-status\n",
                "0".repeat(40),
                "a".repeat(40),
                reference
            );
            format!("{:04x}{payload}0000", payload.len() + 4).into_bytes()
        };
        assert!(validate_push(&command("refs/heads/df1/fix"), "df1/").is_ok());
        assert!(validate_push(&command("refs/heads/fix"), "df1/").is_err());
        assert!(validate_push(&command("refs/tags/v1"), "df1/").is_err());
    }
}
