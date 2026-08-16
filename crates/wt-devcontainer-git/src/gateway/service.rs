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

    pub(super) fn serve_cli(
        &self,
        args: &[String],
        _branch: Option<&str>,
        _head: Option<&str>,
        grant: &GrantRecord,
    ) -> Result<String> {
        if args == ["--help"] || args == ["-h"] || args == ["help"] {
            return Ok(HELP.to_owned());
        }
        let command = api::CliCommand::parse(args)?;
        if let Some((kind, description)) = command.agent_git_report() {
            let world_id = Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
            wt_registry::Registry::open(&self.config.database_path)
                .context("open WT registry")?
                .insert_agent_git_report(world_id, kind, description)
                .context("store agent Git report")?;
            return Ok("Recorded ag-git report for this world.\n".to_owned());
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
            return Ok(cli_unavailable());
        };
        let project = source.path.trim_end_matches(".git");
        let scope = api::ProviderProjectScope {
            host: &source.host,
            project,
            base: &grant.base,
            prefix: &grant.prefix,
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
                message.push_str("Run `ag-git --help` for explicit provider commands.\n");
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
                    let mr = request.handle.trim_start_matches(['#', '!']);
                    message.push_str(&format!(
                        "Updated MR {mr}: {}\nInspect it with:\n  ag-git '{{\"action\":\"show_mr\",\"mr\":{mr}}}'\n  ag-git '{{\"action\":\"list_threads\",\"mr\":{mr}}}'\n  ag-git '{{\"action\":\"list_ci\",\"commit\":\"{head}\"}}'\n",
                        request.url
                    ));
                }
                Ok(api::ProviderCommandOutput::CurrentStatus(None)) => {
                    message.push_str("This branch does not have a pull or merge request.\n");
                    message.push_str(&format!(
                        "Open one with:\n  ag-git '{{\"action\":\"open_mr\",\"head\":\"{branch}\",\"base\":\"{}\"}}'\n",
                        grant.base
                    ));
                }
                Ok(_) | Err(_) => {
                    message.push_str("Run `ag-git --help` for explicit provider commands.\n")
                }
            }
        }
        Ok(message)
    }
}
