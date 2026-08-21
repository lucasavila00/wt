use super::*;

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
            } => self.reserve(&world_id, source.as_deref(), base.as_deref()),
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
                    &TransportResponse::with_message(git_context_header(&source)),
                )?;
                if let Err(error) = self.serve_git(&mut stream, service, &source) {
                    let message = format!("ERR WT Git gateway failed: {error:#}\n");
                    let _ = write_packet(&mut stream, message.as_bytes());
                    let _ = stream.flush();
                }
                Ok(())
            }
            ClientOperation::Cli {
                args,
                repository,
                branch,
                head,
            } => {
                let response = match self.serve_cli(
                    &args,
                    repository.as_ref(),
                    branch.as_deref(),
                    head.as_deref(),
                    &grant,
                ) {
                    Ok(output) => TransportResponse::with_message(output),
                    Err(error) => TransportResponse::error(format!("{error:#}")),
                };
                crate::write_json_line(&mut stream, &response)
            }
        }
    }

    fn reserve(
        &self,
        world_id: &str,
        source: Option<&str>,
        base: Option<&str>,
    ) -> Result<ControlResponse> {
        if world_id.is_empty() || source.is_some() != base.is_some() {
            bail!("invalid gateway grant scope");
        }
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
                return Ok(ControlResponse::ok(Some(Grant {
                    id: existing.id.clone(),
                    token: existing.token.clone(),
                })));
            }
        }
        if let (Some(source), Some(base)) = (source, base) {
            if base.is_empty() {
                bail!("invalid gateway repository check");
            }
            let parsed = parse_source(source)?;
            verify_repository(self.provider(&parsed.host)?, &parsed, base)?;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("gateway state lock poisoned"))?;
        if let Some(existing) = state
            .grants
            .iter()
            .find(|grant| grant.world_id == world_id && !grant.revoked)
        {
            return Ok(ControlResponse::ok(Some(Grant {
                id: existing.id.clone(),
                token: existing.token.clone(),
            })));
        }
        let record = GrantRecord {
            id: Uuid::new_v4().to_string(),
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            world_id: world_id.to_owned(),
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
    ) -> Result<()> {
        let source = parse_source(source)?;
        let provider = self.provider(&source.host)?;
        let policy = WritePolicy::new(format!("refs/heads/{BRANCH_PREFIX}"), [])?;
        let provider_api_available = match provider {
            Provider::Ssh { .. } => true,
            Provider::Local { api, .. } => api.is_some(),
        };
        let message = |commands: &[u8], response: &[u8], sideband: bool| {
            push_result_message(provider_api_available, commands, response, sideband)
        };
        serve_git(
            stream,
            git_target(provider, &source)?,
            service,
            Some(&policy),
            Some(&push_rejection_message),
            Some(&message),
        )
    }

    pub(super) fn serve_cli(
        &self,
        args: &[String],
        repository: Option<&Repository>,
        _branch: Option<&str>,
        _head: Option<&str>,
        grant: &GrantRecord,
    ) -> Result<String> {
        if args == ["--help"] || args == ["-h"] || args == ["help"] {
            return Ok(HELP.to_owned());
        }
        // Setup hook outside the agent-facing JSON API. World builders use this
        // to inject gateway-owned instructions into coding-agent sessions.
        if args == ["world-prompt"] {
            return Ok(world_prompt());
        }
        let command = api::CliCommand::parse(args)?;
        if let Some((kind, description)) = command.wt_tool_report() {
            let world_id = Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
            wt_workload_registry::Registry::open(&self.config.database_path)
                .context("open WT registry")?
                .insert_agent_tool_report(world_id, kind, description)
                .context("store agent tool report")?;
            return Ok("Recorded wt-tools report for this world.\n".to_owned());
        }
        let repository = repository.context(
            "wt-tools needs a Git checkout with an origin to select a repository for this command",
        )?;
        validate_repository(repository)?;
        let provider = self.provider(&repository.host)?;
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
            return Ok(cli_unavailable());
        };
        let scope = api::ProviderProjectScope {
            host: &repository.host,
            project: &repository.project,
            prefix: BRANCH_PREFIX,
        };
        let output = match api_base {
            Some(base) => api::execute_cli_provider_command_at_base(
                kind,
                api_token_file,
                base,
                &scope,
                &command,
            ),
            None => api::execute_cli_provider_command(kind, api_token_file, &scope, &command),
        }?;
        Ok(api::render_cli_command_output(output))
    }
}

pub(super) fn push_rejection_message(violation: &PushViolation) -> String {
    match violation {
        PushViolation::NonBranch { .. } => {
            "tags and non-branch refs cannot be pushed from this environment".to_owned()
        }
        PushViolation::Unauthorized { reference } => {
            let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
            format!(
                "branch `{branch}` must use the shared `{BRANCH_PREFIX}` prefix; rename it with `git branch -m {BRANCH_PREFIX}NAME`"
            )
        }
    }
}

pub(super) fn push_result_message(
    provider_api_available: bool,
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
        if !provider_api_available {
            message.push_str("Run `wt-tools --help` for explicit provider commands.\n");
            continue;
        }
        let show_mr = serde_json::json!({
            "action": "show_mr_for_branch",
            "branch": branch,
        });
        let list_ci = serde_json::json!({
            "action": "list_ci",
            "commit": head,
        });
        message.push_str(&format!(
            "Inspect its open MR with:\n  wt-tools '{show_mr}'\nIf that reports no open MR, run `wt-tools --help` and open one with an explicit base.\nInspect CI with:\n  wt-tools '{list_ci}'\n"
        ));
    }
    Ok(message)
}
