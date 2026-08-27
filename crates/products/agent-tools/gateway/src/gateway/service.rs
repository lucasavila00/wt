use super::*;

impl Gateway {
    pub fn open(config: GatewayConfig) -> Result<Self> {
        if config.providers.is_empty() {
            bail!("at least one Git provider is required");
        }
        let mut hosts = std::collections::BTreeSet::new();
        let mut api_kinds = std::collections::BTreeSet::new();
        for provider in &config.providers {
            if !valid_host(provider.host()) || !hosts.insert(provider.host()) {
                bail!(
                    "invalid or duplicate Git provider host: {}",
                    provider.host()
                );
            }
            if let Some(kind) = provider.api_kind() {
                if !api_kinds.insert(kind) {
                    bail!("duplicate {} API provider", api::provider_name(kind));
                }
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
            ControlRequest::Reserve { world_id } => self.reserve(&world_id),
            ControlRequest::Revoke { grant_id } => self.revoke(&grant_id),
        }
    }

    pub fn reserve_grant(&self, world_id: Uuid) -> Result<Grant> {
        self.reserve(&world_id.to_string())?
            .grant
            .context("gateway reserve response has no grant")
    }

    pub fn revoke_grant(&self, grant_id: &str) -> Result<()> {
        self.revoke(grant_id).map(|_| ())
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
                self.serve_git(&mut stream, service, &source, &grant)
                    .context("serve Git request")
            }
            ClientOperation::Cli { args } => {
                let response = match self.serve_cli(&args, &grant) {
                    Ok(output) => TransportResponse::with_message(output),
                    Err(error) => TransportResponse::error(format!("{error:#}")),
                };
                crate::write_json_line(&mut stream, &response)
            }
            ClientOperation::PaneObservations { panes } => {
                let response = match self.store_pane_observations(&panes, &grant) {
                    Ok(()) => TransportResponse::ok(),
                    Err(error) => TransportResponse::error(format!("{error:#}")),
                };
                crate::write_json_line(&mut stream, &response)
            }
        }
    }

    pub(super) fn store_pane_observations(
        &self,
        panes: &[crate::PaneObservation],
        grant: &GrantRecord,
    ) -> Result<()> {
        let world_id = Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
        let inputs = panes
            .iter()
            .map(|pane| wt_workload_registry::PaneObservationInput {
                tmux_session: &pane.tmux_session,
                pane_id: &pane.pane_id,
                screen_fingerprint: &pane.screen_fingerprint,
                cwd: &pane.cwd,
                git_branch: pane.git_branch.as_deref(),
            })
            .collect::<Vec<_>>();
        wt_workload_registry::Registry::open(&self.config.database_path)
            .context("open WT registry")?
            .replace_pane_observations(world_id.into(), &inputs)
            .context("store pane observations")
    }

    fn reserve(&self, world_id: &str) -> Result<ControlResponse> {
        if world_id.is_empty() {
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

    fn cli_provider(&self, kind: ProviderKind) -> Result<&Provider> {
        self.config
            .providers
            .iter()
            .find(|provider| provider.api_kind() == Some(kind))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} API provider is not configured",
                    api::provider_name(kind)
                )
            })
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
        let policy = WritePolicy::new(format!("refs/heads/{BRANCH_PREFIX}"), [])?;
        let repository = normalize_repository(&source.path);
        let provider_target = provider.api_kind().map(|kind| (kind, repository.as_str()));
        let world_id = Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
        wt_workload_registry::Registry::open(&self.config.database_path)
            .context("open WT registry")?
            .insert_git_activity(wt_workload_registry::GitActivityInput {
                world_id: world_id.into(),
                kind: wt_workload_registry::GitActivityKind::Service,
                provider_host: &source.host,
                repository: &repository,
                git_service: Some(service.command()),
                branch: None,
                previous_oid: None,
                new_oid: None,
            })
            .context("store Git service activity")?;
        let message = |commands: &[u8], response: &[u8], sideband: bool| {
            push_result_message(provider_target, commands, response, sideband)
        };
        let result = serve_git(
            stream,
            git_target(provider, &source)?,
            service,
            Some(&policy),
            Some(&push_rejection_message),
            Some(&message),
        )?;
        if let Some(receive_pack) = result.receive_pack {
            let updates = successful_push_updates(
                &receive_pack.commands,
                &receive_pack.response,
                receive_pack.sideband,
            );
            let updates = match updates {
                Ok(updates) => updates,
                Err(error) => {
                    eprintln!("wt-agent-tool-gateway: inspect successful Git push: {error:#}");
                    Vec::new()
                }
            };
            for update in updates {
                let branch = update
                    .reference
                    .strip_prefix("refs/heads/")
                    .expect("validated successful push is a branch");
                if let Err(error) =
                    self.store_git_activity(wt_workload_registry::GitActivityInput {
                        world_id: world_id.into(),
                        kind: wt_workload_registry::GitActivityKind::BranchUpdate,
                        provider_host: &source.host,
                        repository: &repository,
                        git_service: Some(service.command()),
                        branch: Some(branch),
                        previous_oid: Some(&update.previous_oid),
                        new_oid: Some(&update.new_oid),
                    })
                {
                    eprintln!("wt-agent-tool-gateway: store Git branch activity: {error}");
                }
            }
        }
        Ok(())
    }

    pub(super) fn serve_cli(&self, args: &[String], grant: &GrantRecord) -> Result<String> {
        if args == ["--help"] || args == ["-h"] || args == ["help"] {
            return Ok(wt_tools_help());
        }
        if args == ["world-prompt"] {
            return Ok(world_prompt());
        }
        let parsed = api::WtToolsCommand::parse(args)?;
        let (target, command) = match &parsed {
            api::WtToolsCommand::Feedback { command } => {
                let (kind, description) = command.wt_tool_report();
                let world_id =
                    Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
                wt_workload_registry::Registry::open(&self.config.database_path)
                    .context("open WT registry")?
                    .insert_agent_tool_report(world_id.into(), kind, description)
                    .context("store agent tool report")?;
                return Ok(api::render_cli_confirmation(
                    "Recorded wtg tools report for this world.",
                ));
            }
            api::WtToolsCommand::GitHosting { target, command } => (target, command),
        };
        let repository = normalize_repository(&target.repository);
        validate_repository(&repository)?;
        let provider = self.cli_provider(target.provider)?;
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
            bail!("{}", cli_unavailable().trim());
        };
        let scope = api::ProviderProjectScope {
            host: provider.host(),
            project: &repository,
            prefix: BRANCH_PREFIX,
        };
        let output = match api_base {
            Some(base) => api::execute_cli_provider_command_at_base(
                kind,
                api_token_file,
                base,
                &scope,
                command,
            ),
            None => api::execute_cli_provider_command(kind, api_token_file, &scope, command),
        }?;
        let response_json = api::render_cli_command_output(output);
        let (action, branch, change_request) = wt_tools_activity_metadata(command, &response_json)?;
        let world_id = Uuid::parse_str(&grant.world_id).context("invalid grant world ID")?;
        if let Err(error) =
            self.store_wt_tools_activity(wt_workload_registry::WtToolsActivityInput {
                world_id: world_id.into(),
                provider_host: provider.host(),
                repository: &repository,
                action: &action,
                branch: branch.as_deref(),
                change_request: change_request.as_deref(),
                request_json: &args[0],
                response_json: &response_json,
            })
        {
            eprintln!("wt-agent-tool-gateway: store wtg tools activity: {error}");
        }
        Ok(response_json)
    }

    fn store_git_activity(&self, input: wt_workload_registry::GitActivityInput<'_>) -> Result<()> {
        wt_workload_registry::Registry::open(&self.config.database_path)
            .context("open WT registry")?
            .insert_git_activity(input)
            .context("store Git activity")
    }

    fn store_wt_tools_activity(
        &self,
        input: wt_workload_registry::WtToolsActivityInput<'_>,
    ) -> Result<()> {
        wt_workload_registry::Registry::open(&self.config.database_path)
            .context("open WT registry")?
            .insert_wt_tools_activity(input)
            .context("store wtg tools activity")
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
    provider_target: Option<(ProviderKind, &str)>,
    commands: &[u8],
    response: &[u8],
    sideband: bool,
) -> Result<String> {
    let updates = successful_push_updates(commands, response, sideband)?;
    let mut message = String::new();
    for update in updates {
        let branch = update
            .reference
            .strip_prefix("refs/heads/")
            .expect("validated successful push is a branch");
        if update.new_oid.bytes().all(|byte| byte == b'0') {
            message.push_str(&format!("Deleted branch `{branch}`.\n"));
            continue;
        }
        message.push_str(&format!("Published branch `{branch}`.\n"));
        let Some((provider, repository)) = provider_target else {
            message.push_str("Run `wtg tools --help` for explicit provider commands.\n");
            continue;
        };
        let show_mr = serde_json::json!({
            "target": { "provider": provider, "repository": repository },
            "command": { "action": "show_mr_for_branch", "branch": branch },
        });
        let list_ci = serde_json::json!({
            "target": { "provider": provider, "repository": repository },
            "command": { "action": "list_ci", "commit": update.new_oid },
        });
        message.push_str(&format!(
            "Inspect its open MR with:\n  wtg tools '{show_mr}'\nIf that reports no open MR, run `wtg tools --help` and open one with an explicit base.\nInspect CI with:\n  wtg tools '{list_ci}'\n"
        ));
    }
    Ok(message)
}

pub(super) fn wt_tools_activity_metadata(
    command: &api::GitHostingCommand,
    response_json: &str,
) -> Result<(String, Option<String>, Option<String>)> {
    let (action, branch, change_request) = match command {
        api::GitHostingCommand::ShowMr { mr } => ("show_mr", None, Some(mr.as_str())),
        api::GitHostingCommand::ShowMrForBranch { branch } => {
            ("show_mr_for_branch", Some(branch.as_str()), None)
        }
        api::GitHostingCommand::ShowRun { .. } => ("show_run", None, None),
        api::GitHostingCommand::ShowJob { .. } => ("show_job", None, None),
        api::GitHostingCommand::ListThreads { mr } => ("list_threads", None, Some(mr.as_str())),
        api::GitHostingCommand::ListComments { mr } => ("list_comments", None, Some(mr.as_str())),
        api::GitHostingCommand::ShowComment { mr, .. } => ("show_comment", None, Some(mr.as_str())),
        api::GitHostingCommand::EditComment { mr, .. } => ("edit_comment", None, Some(mr.as_str())),
        api::GitHostingCommand::DeleteComment { mr, .. } => {
            ("delete_comment", None, Some(mr.as_str()))
        }
        api::GitHostingCommand::ListCi { .. } => ("list_ci", None, None),
        api::GitHostingCommand::ListJobs { .. } => ("list_jobs", None, None),
        api::GitHostingCommand::LogJob { .. } => ("log_job", None, None),
        api::GitHostingCommand::WaitMr { mr, .. } => ("wait_mr", None, Some(mr.as_str())),
        api::GitHostingCommand::WaitRun { .. } => ("wait_run", None, None),
        api::GitHostingCommand::WaitJob { .. } => ("wait_job", None, None),
        api::GitHostingCommand::OpenMr { head, .. } => ("open_mr", Some(head.as_str()), None),
        api::GitHostingCommand::SetMr { mr, .. } => ("set_mr", None, Some(mr.as_str())),
        api::GitHostingCommand::EditMr { mr, .. } => ("edit_mr", None, Some(mr.as_str())),
        api::GitHostingCommand::CommentMr { mr, .. } => ("comment_mr", None, Some(mr.as_str())),
        api::GitHostingCommand::ReplyThread { mr, .. } => ("reply_thread", None, Some(mr.as_str())),
        api::GitHostingCommand::SetThread { mr, .. } => ("set_thread", None, Some(mr.as_str())),
        api::GitHostingCommand::RetryJob { .. } => ("retry_job", None, None),
        api::GitHostingCommand::CancelJob { .. } => ("cancel_job", None, None),
        api::GitHostingCommand::CancelRun { .. } => ("cancel_run", None, None),
    };
    let response = serde_json::from_str::<serde_json::Value>(response_json)
        .context("decode wtg tools response JSON")?;
    let data = response.get("data");
    let branch = branch.map(str::to_owned).or_else(|| {
        data.and_then(|value| value.get("head"))?
            .as_str()
            .map(str::to_owned)
    });
    let change_request = change_request.map(str::to_owned).or_else(|| {
        data.and_then(|value| value.get("handle"))?
            .as_str()
            .map(str::to_owned)
    });
    Ok((action.to_owned(), branch, change_request))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_prompt_does_not_require_a_provider_api() {
        let temp = tempfile::tempdir().unwrap();
        let gateway = Gateway::open(GatewayConfig {
            state_file: temp.path().join("gateway.json"),
            database_path: temp.path().join("instances.db"),
            providers: vec![Provider::Local {
                host: "github.com".into(),
                repositories: temp.path().to_owned(),
                api: None,
            }],
        })
        .unwrap();
        let grant = GrantRecord {
            id: "id".into(),
            token: "token".into(),
            world_id: "world".into(),
            revoked: false,
        };

        assert_eq!(
            gateway.serve_cli(&["world-prompt".into()], &grant).unwrap(),
            world_prompt()
        );
    }
}
