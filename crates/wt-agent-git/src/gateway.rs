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
            } => self.reserve(&world_id, &source, &base),
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
        match request.operation {
            ClientOperation::Git { service, source } => {
                crate::write_json_line(
                    &mut stream,
                    &TransportResponse::with_message(git_context_header(&grant)),
                )?;
                if let Err(error) = self.serve_git(&mut stream, service, &source, &grant) {
                    let message = format!("ERR WT Git gateway failed: {error:#}\n");
                    let _ = write_packet(&mut stream, message.as_bytes());
                    let _ = stream.flush();
                }
                Ok(())
            }
            ClientOperation::Cli { args, branch, head } => {
                let response =
                    match self.serve_cli(&args, branch.as_deref(), head.as_deref(), &grant) {
                        Ok(output) => TransportResponse::with_message(output),
                        Err(error) => TransportResponse::error(format!("{error:#}")),
                    };
                crate::write_json_line(&mut stream, &response)
            }
        }
    }

    fn reserve(&self, world_id: &str, source: &str, base: &str) -> Result<ControlResponse> {
        if world_id.is_empty() || base.is_empty() {
            bail!("invalid gateway grant scope");
        }
        let parsed = parse_source(source)?;
        let provider = self.provider(&parsed.host)?;
        {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
            if let Some(existing) = state
                .grants
                .iter()
                .find(|grant| grant.world_id == world_id && !grant.revoked)
            {
                if existing.source != source || existing.base != base {
                    bail!("world already reserved with a different Git scope");
                }
                return Ok(ControlResponse::ok(Some(Grant {
                    id: existing.id.clone(),
                    token: existing.token.clone(),
                })));
            }
        }
        verify_repository(provider, &parsed, base)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
        if let Some(existing) = state
            .grants
            .iter()
            .find(|grant| grant.world_id == world_id && !grant.revoked)
        {
            if existing.source != source || existing.base != base {
                bail!("world already reserved with a different Git scope");
            }
            return Ok(ControlResponse::ok(Some(Grant {
                id: existing.id.clone(),
                token: existing.token.clone(),
            })));
        }
        let record = GrantRecord {
            id: Uuid::new_v4().to_string(),
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            world_id: world_id.to_owned(),
            source: source.to_owned(),
            base: base.to_owned(),
            prefix: BRANCH_PREFIX.to_owned(),
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
        if grant.revoked {
            return Ok(ControlResponse::ok(None));
        }
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
        stream: &mut S,
        service: GitService,
        source: &str,
        grant: &GrantRecord,
    ) -> Result<()> {
        let source = parse_source(source)?;
        let provider = self.provider(&source.host)?;
        let mut child = spawn_git(provider, &source, service)?;
        if service == GitService::ReceivePack {
            forward_advertisement(&mut child, stream)?;
            let commands = read_packet_section(&mut *stream)?;
            if let Err(error) = validate_push(&commands, &grant.prefix) {
                reject_push(stream, &commands, &error.to_string())?;
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
                self.push_result_message(provider, &source, grant, &commands, response, sideband)
            };
            return bridge_child(
                stream,
                child,
                sideband.then_some(&message as &dyn Fn(&[u8]) -> Result<String>),
            );
        }
        forward_advertisement(&mut child, stream)?;
        bridge_child(stream, child, None)
    }

    fn serve_cli(
        &self,
        args: &[String],
        branch: Option<&str>,
        head: Option<&str>,
        grant: &GrantRecord,
    ) -> Result<String> {
        if args == ["--help"] || args == ["-h"] || args == ["help"] {
            return Ok(HELP.to_owned());
        }
        let source = parse_source(&grant.source)?;
        let provider = self.provider(&source.host)?;
        let api = match provider {
            Provider::Ssh {
                kind,
                api_token_file,
                ..
            } => Some((*kind, api_token_file, None)),
            Provider::Local { api, .. } => api
                .as_ref()
                .map(|api| (api.kind, &api.token_file, Some(api.base_url.as_str()))),
        };
        let Some((kind, api_token_file, api_base)) = api else {
            return Ok(cli_output(args, branch, grant));
        };
        let branch = branch.context("ag-git requires a branch checkout")?;
        let head = head.context("ag-git requires a commit checkout")?;
        if !branch.starts_with(&grant.prefix) || branch.len() == grant.prefix.len() {
            bail!(
                "branch `{branch}` must use the shared `{}` prefix; rename it with `git branch -m {}NAME`",
                grant.prefix,
                grant.prefix
            );
        }
        if !valid_object_id(head) {
            bail!("current Git commit is invalid");
        }
        let published_head = repository_refs(provider, &source)?
            .into_iter()
            .find_map(|(object, reference)| {
                (reference == format!("refs/heads/{branch}")).then_some(object)
            })
            .with_context(|| {
                format!(
                    "branch `{branch}` is not published; run `git push -u origin {branch}` and retry"
                )
            })?;
        if published_head != head {
            bail!(
                "branch `{branch}` is published at {published_head}, but this checkout is at {head}; run `git push origin {branch}` and retry"
            );
        }
        let project = source.path.trim_end_matches(".git");
        let command = api::ProviderCommand::parse(args)?;
        let scope = api::ProviderCommandScope {
            host: &source.host,
            project,
            base: &grant.base,
            prefix: &grant.prefix,
            branch,
            head,
        };
        let output = match api_base {
            Some(base) => {
                api::execute_provider_command_at_base(kind, api_token_file, base, &scope, &command)
            }
            None => api::execute_provider_command(kind, api_token_file, &scope, &command),
        }?;
        Ok(api::render_provider_command_output(output, &scope))
    }

    fn push_result_message(
        &self,
        provider: &Provider,
        source: &GitSource,
        grant: &GrantRecord,
        commands: &[u8],
        response: &[u8],
        sideband: bool,
    ) -> Result<String> {
        let updates = successful_push_updates(commands, response, sideband)?;
        let mut message = String::new();
        for (head, branch) in updates {
            if head.bytes().all(|byte| byte == b'0') {
                message.push_str(&format!("Deleted branch `{branch}`.\n"));
                continue;
            }
            message.push_str(&format!("Published branch `{branch}`.\n"));
            let api = match provider {
                Provider::Ssh {
                    kind,
                    api_token_file,
                    ..
                } => Some((*kind, api_token_file, None)),
                Provider::Local { api, .. } => api
                    .as_ref()
                    .map(|api| (api.kind, &api.token_file, Some(api.base_url.as_str()))),
            };
            let Some((kind, api_token_file, api_base)) = api else {
                message
                    .push_str("Run `ag-git` to see its pull or merge request, reviews, and CI.\n");
                message.push_str("If it has no request, open one with `ag-git open-mr`.\n");
                continue;
            };
            let scope = api::ProviderCommandScope {
                host: &source.host,
                project: source.path.trim_end_matches(".git"),
                base: &grant.base,
                prefix: &grant.prefix,
                branch: &branch,
                head: &head,
            };
            let result = match api_base {
                Some(base) => api::execute_provider_command_at_base(
                    kind,
                    api_token_file,
                    base,
                    &scope,
                    &api::ProviderCommand::ReadChangeRequestAfterPush,
                ),
                None => api::execute_provider_command(
                    kind,
                    api_token_file,
                    &scope,
                    &api::ProviderCommand::ReadChangeRequestAfterPush,
                ),
            };
            match result {
                Ok(api::ProviderCommandOutput::CurrentStatus(Some(request))) => {
                    message.push_str(&format!(
                        "Updated request {}: {}\nRun `ag-git` to see review comments and CI.\n",
                        request.handle, request.url
                    ));
                }
                Ok(api::ProviderCommandOutput::CurrentStatus(None)) => {
                    message.push_str("This branch does not have a pull or merge request.\n");
                    message.push_str("Open one for review:\n  ag-git open-mr\n");
                    message.push_str("Or open it as a draft:\n  ag-git open-mr --draft\n");
                }
                Ok(_) | Err(_) => message
                    .push_str("Run `ag-git` to see its pull or merge request, reviews, and CI.\n"),
            }
        }
        Ok(message)
    }
}

fn cli_output(args: &[String], branch: Option<&str>, grant: &GrantRecord) -> String {
    if args == ["--help"] || args == ["-h"] || args == ["help"] {
        HELP.to_owned()
    } else if args.is_empty() {
        format!(
            "WT agent Git\n\nProject: {}\nCurrent branch: {}\nShared branch prefix: {}\nRequest base: {}\n\nPull or merge request, review, and CI commands are not available in this build yet.\nNormal Git fetch, pull, and push are available now.\nRun `ag-git --help` to see the command contract.\n",
            grant.source,
            branch.unwrap_or("detached HEAD"),
            grant.prefix,
            grant.base,
        )
    } else {
        "ag-git: pull or merge request, review, and CI commands are not available in this build yet.\nNormal Git fetch, pull, and push are available now.\nRun `ag-git --help` to see the command contract.\n".to_owned()
    }
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
remote: ag-git is the installed CLI for pull or merge requests, reviews, and CI.\n\
remote: Run ag-git for the current branch's status and suggested next actions.\n\
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
    edit [--title TEXT] [--body TEXT]\n\
    review                  Show review threads and their handles\n\
    reply HANDLE TEXT       Reply to a review thread\n\
    resolve HANDLE          Resolve a review thread\n\
    reopen HANDLE           Reopen a review thread\n\
    ci                      Show CI jobs for the current commit\n\
    log JOB                 Show any CI job log in this project by provider ID\n\
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
        assert!(validate_push(&command("refs/heads/wt/fix"), "wt/").is_ok());
        assert!(validate_push(&command("refs/heads/fix"), "wt/").is_err());
        assert!(validate_push(&command("refs/tags/v1"), "wt/").is_err());
    }

    #[test]
    fn help_is_the_complete_command_contract() {
        insta::assert_snapshot!(HELP);
    }

    #[test]
    fn git_header_explains_the_environment_without_prior_context() {
        let grant = test_grant();
        insta::assert_snapshot!(git_context_header(&grant));
    }

    #[test]
    fn cli_status_and_unavailable_command_are_actionable() {
        let grant = test_grant();
        insta::assert_snapshot!("cli_status", cli_output(&[], Some("wt/fix-login"), &grant));
        insta::assert_snapshot!(
            "cli_unavailable",
            cli_output(&["open-mr".to_owned()], Some("wt/fix-login"), &grant)
        );
    }

    #[test]
    fn push_messages_cover_publish_delete_and_rejection() {
        let command = |new: &str, reference: &str| {
            let payload = format!("{} {new} {reference}\0report-status\n", "0".repeat(40));
            format!("{:04x}{payload}0000", payload.len() + 4).into_bytes()
        };
        let response = |status: &str| {
            let mut report = Vec::new();
            write_packet(&mut report, b"unpack ok\n").unwrap();
            write_packet(&mut report, format!("{status}\n").as_bytes()).unwrap();
            report.extend_from_slice(b"0000");
            let mut packet = vec![1];
            packet.extend_from_slice(&report);
            let mut response = Vec::new();
            write_packet(&mut response, &packet).unwrap();
            response.extend_from_slice(b"0000");
            response
        };
        assert_eq!(
            successful_push_updates(
                &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
                &response("ok refs/heads/wt/fix-login"),
                true,
            )
            .unwrap(),
            vec![("a".repeat(40), "wt/fix-login".to_owned())]
        );
        assert_eq!(
            successful_push_updates(
                &command(&"0".repeat(40), "refs/heads/wt/fix-login"),
                &response("ok refs/heads/wt/fix-login"),
                true,
            )
            .unwrap(),
            vec![("0".repeat(40), "wt/fix-login".to_owned())]
        );
        assert!(successful_push_updates(
            &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
            &response("ng refs/heads/wt/fix-login protected branch"),
            true,
        )
        .unwrap()
        .is_empty());
        insta::assert_snapshot!(
            "push_rejected",
            validate_push(&command(&"a".repeat(40), "refs/heads/fix-login"), "wt/")
                .unwrap_err()
                .to_string()
        );
    }

    fn test_grant() -> GrantRecord {
        GrantRecord {
            id: "id".to_owned(),
            token: "token".to_owned(),
            world_id: "world".to_owned(),
            source: "git@github.com:group/project.git".to_owned(),
            base: "main".to_owned(),
            prefix: BRANCH_PREFIX.to_owned(),
            revoked: false,
        }
    }
}
