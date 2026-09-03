use super::*;

impl Gateway {
    pub fn open(config: GatewayConfig, activity: ActivityRecorder) -> Result<Self> {
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
        Ok(Self {
            config,
            activity,
            world_state: Arc::new(Mutex::new(WorldState::default())),
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
            ControlRequest::ActivateWorld { world_id } => {
                self.activate_world(parse_world_id(&world_id)?)?;
                Ok(ControlResponse::ok())
            }
            ControlRequest::DeactivateWorld { world_id } => {
                self.deactivate_world(parse_world_id(&world_id)?)?;
                Ok(ControlResponse::ok())
            }
        }
    }

    pub fn handle_transport<S: DuplexStream>(
        &self,
        mut stream: S,
        world_id: WorldId,
    ) -> Result<()> {
        let request: TransportRequest = crate::read_json_line(&mut stream)?;
        let authorized = match self.authorize(&request, world_id) {
            Ok(authorized) => authorized,
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
                self.serve_git(&mut stream, service, &source, authorized.world_id)
                    .context("serve Git request")
            }
            ClientOperation::Cli { args } => {
                let response = match self.serve_cli(&args, authorized.world_id) {
                    Ok(output) => TransportResponse::with_message(output),
                    Err(error) => TransportResponse::error(format!("{error:#}")),
                };
                crate::write_json_line(&mut stream, &response)
            }
            ClientOperation::SendMessageToParent { message } => {
                let response = match self
                    .activity
                    .record_world_mail(authorized.world_id, &message)
                {
                    Ok(()) => TransportResponse::with_message(api::render_cli_confirmation(
                        "Sent message to parent.".to_owned(),
                    )),
                    Err(error) => TransportResponse::error(format!("{error:#}")),
                };
                crate::write_json_line(&mut stream, &response)
            }
            ClientOperation::PaneObservations { panes } => {
                let response = match self.store_pane_observations(&panes, &authorized) {
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
        authorized: &AuthorizedWorld,
    ) -> Result<()> {
        crate::protocol::validate_pane_observations(panes).map_err(anyhow::Error::msg)?;
        let observed_at_unix_ms = now_unix_ms()?;
        let mut observations = self
            .world_state
            .lock()
            .map_err(|_| anyhow::anyhow!("pane observation lock poisoned"))?;
        let world_id = authorized.world_id;
        if observations
            .generations
            .get(&world_id)
            .copied()
            .unwrap_or_default()
            != authorized.pane_generation
        {
            bail!("pane observation belongs to an expired world run");
        }
        if observations.inactive_worlds.contains(&world_id) {
            bail!("pane observations are inactive for this world");
        }
        replace_pane_observations(
            &mut observations.snapshots,
            world_id,
            panes,
            observed_at_unix_ms,
        );
        Ok(())
    }

    pub fn pane_observations(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<crate::PaneObservationSnapshot>> {
        Ok(self
            .world_state
            .lock()
            .map_err(|_| anyhow::anyhow!("pane observation lock poisoned"))?
            .snapshots
            .get(&world_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn activate_world(&self, world_id: WorldId) -> Result<()> {
        let mut observations = self
            .world_state
            .lock()
            .map_err(|_| anyhow::anyhow!("pane observation lock poisoned"))?;
        if observations.inactive_worlds.remove(&world_id) {
            advance_pane_generation(&mut observations, world_id);
        }
        Ok(())
    }

    pub fn deactivate_world(&self, world_id: WorldId) -> Result<()> {
        let mut observations = self
            .world_state
            .lock()
            .map_err(|_| anyhow::anyhow!("pane observation lock poisoned"))?;
        if observations.inactive_worlds.insert(world_id) {
            advance_pane_generation(&mut observations, world_id);
        }
        observations.snapshots.remove(&world_id);
        Ok(())
    }

    fn authorize(&self, request: &TransportRequest, world_id: WorldId) -> Result<AuthorizedWorld> {
        if request.protocol_version != PROTOCOL_VERSION {
            bail!(
                "unsupported gateway protocol version {}; expected {PROTOCOL_VERSION}",
                request.protocol_version
            );
        }
        let observations = self
            .world_state
            .lock()
            .map_err(|_| anyhow::anyhow!("pane observation lock poisoned"))?;
        if observations.inactive_worlds.contains(&world_id) {
            bail!("agent tools are inactive for this world");
        }
        if let ClientOperation::SendMessageToParent { message } = &request.operation {
            if message.is_empty() || message.len() > wt_workload_registry::MAX_MAIL_MESSAGE_BYTES {
                bail!(
                    "message must contain 1 to {} UTF-8 bytes",
                    wt_workload_registry::MAX_MAIL_MESSAGE_BYTES
                );
            }
        }
        let pane_generation =
            if matches!(&request.operation, ClientOperation::PaneObservations { .. }) {
                observations
                    .generations
                    .get(&world_id)
                    .copied()
                    .unwrap_or_default()
            } else {
                0
            };
        Ok(AuthorizedWorld {
            world_id,
            pane_generation,
        })
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
        world_id: WorldId,
    ) -> Result<()> {
        let source = parse_source(source)?;
        let provider = self.provider(&source.host)?;
        let policy = WritePolicy::new(format!("refs/heads/{BRANCH_PREFIX}"), [])?;
        let repository = normalize_repository(&source.path);
        let provider_target = provider.api_kind().map(|kind| (kind, repository.as_str()));
        self.activity
            .record_git_activity(wt_workload_registry::GitActivityInput {
                world_id,
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
                if let Err(error) = self
                    .activity
                    .record_git_activity(wt_workload_registry::GitActivityInput {
                        world_id,
                        kind: wt_workload_registry::GitActivityKind::BranchUpdate,
                        provider_host: &source.host,
                        repository: &repository,
                        git_service: Some(service.command()),
                        branch: Some(branch),
                        previous_oid: Some(&update.previous_oid),
                        new_oid: Some(&update.new_oid),
                    })
                    .context("store Git activity")
                {
                    eprintln!("wt-agent-tool-gateway: store Git branch activity: {error}");
                }
            }
        }
        Ok(())
    }

    pub(super) fn serve_cli(&self, args: &[String], world_id: WorldId) -> Result<String> {
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
                self.activity
                    .record_agent_tool_report(world_id, kind, description)
                    .context("store agent tool report")?;
                return Ok(api::render_cli_confirmation(
                    "Recorded wtg tools report for this world.",
                ));
            }
            api::WtToolsCommand::World { .. } => {
                bail!("send_message_to_parent must be sent by the guest relay")
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
        if let Err(error) = self
            .activity
            .record_wt_tools_activity(wt_workload_registry::WtToolsActivityInput {
                world_id,
                provider_host: provider.host(),
                repository: &repository,
                action: &action,
                branch: branch.as_deref(),
                change_request: change_request.as_deref(),
                request_json: &args[0],
                response_json: &response_json,
            })
            .context("store wtg tools activity")
        {
            eprintln!("wt-agent-tool-gateway: store wtg tools activity: {error}");
        }
        Ok(response_json)
    }
}

fn replace_pane_observations(
    observations: &mut std::collections::BTreeMap<
        wt_world::WorldId,
        Vec<crate::PaneObservationSnapshot>,
    >,
    world_id: wt_world::WorldId,
    panes: &[crate::PaneObservation],
    observed_at_unix_ms: i64,
) {
    if panes.is_empty() {
        observations.remove(&world_id);
        return;
    }
    let existing = observations.get(&world_id);
    let replacements = panes
        .iter()
        .map(|pane| {
            let changed_at_unix_ms = existing
                .and_then(|existing| {
                    existing.iter().find(|snapshot| {
                        snapshot.tmux_session == pane.tmux_session
                            && snapshot.pane_id == pane.pane_id
                            && snapshot.screen_fingerprint == pane.screen_fingerprint
                    })
                })
                .map_or(observed_at_unix_ms, |snapshot| snapshot.changed_at_unix_ms);
            crate::PaneObservationSnapshot {
                tmux_session: pane.tmux_session.clone(),
                pane_id: pane.pane_id.clone(),
                screen_fingerprint: pane.screen_fingerprint.clone(),
                cwd: pane.cwd.clone(),
                git_branch: pane.git_branch.clone(),
                changed_at_unix_ms,
                observed_at_unix_ms,
                render: wt_control_protocol::PaneRender {
                    window_index: pane.window_index,
                    window_name: pane.window_name.clone(),
                    frame: pane.frame.clone(),
                },
            }
        })
        .collect();
    observations.insert(world_id, replacements);
}

fn advance_pane_generation(observations: &mut WorldState, world_id: WorldId) {
    let generation = observations.generations.entry(world_id).or_default();
    *generation = generation.wrapping_add(1);
}

fn now_unix_ms() -> Result<i64> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system time is before Unix epoch")?
            .as_millis(),
    )
    .context("system time is too large")
}

fn parse_world_id(world_id: &str) -> Result<WorldId> {
    Ok(Uuid::parse_str(world_id)
        .context("invalid world ID")?
        .into())
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
mod tests;
