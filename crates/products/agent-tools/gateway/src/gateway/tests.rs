use super::*;
use crate::CodexGitContext;
use crate::{CodexSessionStartSource, CodexSessionStartSourceKind};
use diesel::prelude::*;
use wt_git_smart_protocol::PushViolation;
use wt_workload_registry::schema::worlds;

#[test]
fn help_is_the_complete_command_contract() {
    insta::assert_snapshot!(wt_tools_help());
}

#[test]
fn git_header_explains_the_environment_without_prior_context() {
    insta::assert_snapshot!(git_context_header("git@github.com:group/project.git"));
}

#[test]
fn cli_status_and_unavailable_command_are_actionable() {
    insta::assert_snapshot!("cli_unavailable", cli_unavailable());
}

#[test]
fn worst_case_ci_log_tail_fits_the_transport_header() {
    let output = api::render_cli_command_output(api::ProviderCommandOutput::CiJobLog {
        log: "\0".repeat(1024 * 1024),
        truncated: true,
    });
    let mut encoded = Vec::new();

    crate::write_json_line(&mut encoded, &TransportResponse::with_message(output)).unwrap();

    assert!(encoded.len() < 8 * 1024 * 1024);
    let decoded: TransportResponse = crate::read_json_line(&mut encoded.as_slice()).unwrap();
    assert!(decoded.ok);
}

#[test]
fn provider_targets_are_validated_and_unambiguous() {
    assert!(validate_repository("acme/widget").is_ok());
    assert_eq!(normalize_repository("acme/widget.git"), "acme/widget");
    for invalid in ["", "/acme/widget", "acme/../widget"] {
        assert!(validate_repository(invalid).is_err());
    }

    let temp = tempfile::tempdir().unwrap();
    let provider = |host: &str| Provider::Local {
        host: host.into(),
        repositories: temp.path().to_owned(),
        api: Some(FixtureApi {
            kind: ProviderKind::GitHub,
            base_url: "http://github.test".into(),
            token_file: temp.path().join("token"),
        }),
    };
    let error = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path: temp.path().join("instances.db"),
        providers: vec![provider("github.test"), provider("enterprise.test")],
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "duplicate GitHub API provider");
}

#[test]
fn wt_tools_activity_metadata_uses_the_head_and_response_handle() {
    let command = api::GitHostingCommand::OpenMr {
        head: "wt/activity".into(),
        base: "main".into(),
    };
    let metadata = service::wt_tools_activity_metadata(
        &command,
        r#"{"type":"change_request","data":{"handle":"42","head":"wt/activity"}}"#,
    )
    .unwrap();
    insta::assert_snapshot!(format!("{} {:?} {:?}", metadata.0, metadata.1, metadata.2), @r###"
    open_mr Some("wt/activity") Some("42")
    "###);

    for (command, expected_action) in [
        (
            api::GitHostingCommand::ListComments { mr: "7".into() },
            "list_comments",
        ),
        (
            api::GitHostingCommand::ShowComment {
                mr: "7".into(),
                comment: "123".into(),
            },
            "show_comment",
        ),
        (
            api::GitHostingCommand::EditComment {
                mr: "7".into(),
                comment: "123".into(),
                body: "Updated".into(),
                confirm_merged: false,
            },
            "edit_comment",
        ),
        (
            api::GitHostingCommand::DeleteComment {
                mr: "7".into(),
                comment: "123".into(),
                confirm_merged: false,
            },
            "delete_comment",
        ),
    ] {
        assert_eq!(
            service::wt_tools_activity_metadata(&command, r#"{"type":"confirmation","data":"ok"}"#)
                .unwrap(),
            (expected_action.to_owned(), None, Some("7".to_owned()))
        );
    }
}

#[test]
fn world_prompt_does_not_require_a_checkout_or_provider_api() {
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

    let output = gateway
        .serve_cli(&["world-prompt".into()], &test_grant())
        .unwrap();

    insta::assert_snapshot!(output);
}

#[test]
fn gateway_state_rejects_unknown_grant_fields() {
    let state = serde_json::from_str::<State>(
        r#"{"grants":[{"id":"id","token":"token","world_id":"world","revoked":false,"unexpected":true}]}"#,
    );
    assert!(state.is_err());
}

#[test]
fn agent_tool_reports_are_stored_for_the_authenticated_world_without_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
    let world_id = insert_world(&registry);
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path,
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();
    let mut grant = test_grant();
    grant.world_id = world_id.to_string();

    for (name, command) in [
        (
            "report_wt_tool_bug",
            r#"{"action":"report_wt_tool_bug","description":"job logs disappear"}"#,
        ),
        (
            "report_wt_tool_issue",
            r#"{"action":"report_wt_tool_issue","description":"the hint is unclear"}"#,
        ),
        (
            "suggest_wt_tool_improvement",
            r#"{"action":"suggest_wt_tool_improvement","description":"show the check name"}"#,
        ),
        (
            "request_wt_tool_feature",
            r#"{"action":"request_wt_tool_feature","description":"support CI search"}"#,
        ),
    ] {
        let output = gateway
            .serve_cli(&[format!(r#"{{"command":{command}}}"#)], &grant)
            .unwrap();
        insta::assert_snapshot!(name, output);
    }
    let reports = registry.list_agent_tool_reports("alice").unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|report| report.world_id == world_id));
    assert_eq!(
        reports.iter().map(|report| report.kind).collect::<Vec<_>>(),
        vec![
            wt_workload_registry::AgentToolReportKind::Bug,
            wt_workload_registry::AgentToolReportKind::Issue,
            wt_workload_registry::AgentToolReportKind::Improvement,
            wt_workload_registry::AgentToolReportKind::FeatureRequest,
        ]
    );
    assert_eq!(reports[0].description, "job logs disappear");
}

#[test]
fn compaction_preserves_the_primary_session_state() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
    let world_id = insert_world(&registry);
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path,
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();
    let mut grant = test_grant();
    grant.world_id = world_id.to_string();
    let session_id = Uuid::new_v4();
    let mut event = CodexSessionEvent {
        session_id,
        cwd: "/home/wt/project".into(),
        tmux_session: "wt-host".into(),
        pane_id: "%3".into(),
        kind: CodexSessionEventKind::UserPromptSubmit,
        pane_generation: 1,
        pane_sequence: 1,
        session_start_source: None,
    };

    gateway.store_codex_session_event(&event, &grant).unwrap();
    event.kind = CodexSessionEventKind::PreCompact;
    event.pane_sequence += 1;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::Working
    );
    assert!(reports[0].is_compacting);

    event.kind = CodexSessionEventKind::SessionStart;
    event.pane_sequence += 1;
    event.session_start_source = Some(CodexSessionStartSource {
        kind: CodexSessionStartSourceKind::Compact,
        raw: "compact".into(),
    });
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::Working
    );
    assert!(reports[0].is_compacting);
    assert_eq!(reports[0].session_start_source.as_deref(), Some("compact"));

    event.kind = CodexSessionEventKind::PostCompact;
    event.pane_sequence += 1;
    event.session_start_source = None;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::Working
    );
    assert!(!reports[0].is_compacting);
    assert_eq!(reports[0].session_start_source, None);

    event.kind = CodexSessionEventKind::Stop;
    event.pane_sequence += 1;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].world_id, world_id);
    assert_eq!(reports[0].session_id, session_id);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::NeedsAttention
    );
    assert!(!reports[0].is_compacting);
    assert_eq!(reports[0].tmux_session, "wt-host");
    assert_eq!(reports[0].pane_id, "%3");
    assert_eq!(reports[0].session_start_source, None);

    event.kind = CodexSessionEventKind::PreCompact;
    event.pane_sequence += 1;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::NeedsAttention
    );
    assert!(reports[0].is_compacting);

    event.kind = CodexSessionEventKind::PostCompact;
    event.pane_sequence += 1;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::NeedsAttention
    );
    assert!(!reports[0].is_compacting);
}

#[test]
fn git_context_updates_only_the_matching_active_report() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
    let world_id = insert_world(&registry);
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path,
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();
    let mut grant = test_grant();
    grant.world_id = world_id.to_string();
    let session_id = Uuid::new_v4();
    let event = CodexSessionEvent {
        session_id,
        cwd: "/home/wt/project".into(),
        tmux_session: "wt-host".into(),
        pane_id: "%4".into(),
        kind: CodexSessionEventKind::UserPromptSubmit,
        pane_generation: 1,
        pane_sequence: 1,
        session_start_source: None,
    };
    assert!(gateway.store_codex_session_event(&event, &grant).unwrap());
    let before = registry
        .list_codex_session_reports("alice")
        .unwrap()
        .remove(0);

    assert!(gateway
        .store_codex_git_context(
            &CodexGitContext {
                session_id,
                cwd: event.cwd.clone(),
                tmux_session: event.tmux_session.clone(),
                pane_id: event.pane_id.clone(),
                pane_generation: event.pane_generation,
                repository_root: Some(event.cwd.clone()),
                repository_url: Some("git@github.com:acme/project.git".into()),
                git_branch: Some("wt/after-switch".into()),
                error: None,
            },
            &grant,
        )
        .unwrap());
    let after = registry
        .list_codex_session_reports("alice")
        .unwrap()
        .remove(0);
    assert_eq!(
        after
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.branch.as_deref()),
        Some("wt/after-switch")
    );
    assert_eq!(
        after
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.provider_host.as_deref()),
        Some("github.com")
    );
    assert_eq!(
        after
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.repository.as_deref()),
        Some("acme/project")
    );
    assert_eq!(after.received_at_unix_ms, before.received_at_unix_ms);
    assert_eq!(
        after.state,
        wt_workload_registry::CodexSessionState::Working
    );
    assert!(after.checkout.is_some());
    let state = registry
        .repository_git_state("alice", "github.com", "acme/project", None, None)
        .unwrap()
        .unwrap();
    assert_eq!(state.checkouts.len(), 1);
    assert_eq!(
        state.checkouts[0].checkout.branch.as_deref(),
        Some("wt/after-switch")
    );

    assert!(gateway
        .store_codex_session_event(
            &CodexSessionEvent {
                pane_generation: 2,
                pane_sequence: 2,
                ..event.clone()
            },
            &grant,
        )
        .unwrap());
    assert!(gateway
        .store_codex_git_context(
            &CodexGitContext {
                pane_generation: 2,
                repository_root: None,
                repository_url: None,
                git_branch: None,
                error: Some("Git state command timed out".into()),
                ..CodexGitContext {
                    session_id,
                    cwd: event.cwd.clone(),
                    tmux_session: event.tmux_session.clone(),
                    pane_id: event.pane_id.clone(),
                    pane_generation: event.pane_generation,
                    repository_root: None,
                    repository_url: None,
                    git_branch: None,
                    error: None,
                }
            },
            &grant,
        )
        .unwrap());
    let failed = registry
        .list_codex_session_reports("alice")
        .unwrap()
        .remove(0);
    assert_eq!(
        failed
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.branch.as_deref()),
        Some("wt/after-switch")
    );
    assert_eq!(
        failed
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.error.as_deref()),
        Some("Git state command timed out")
    );

    assert!(gateway
        .store_codex_session_event(
            &CodexSessionEvent {
                session_id,
                cwd: "/home/wt/project".into(),
                tmux_session: "wt-host".into(),
                pane_id: "%4".into(),
                kind: CodexSessionEventKind::SessionEnd,
                pane_generation: 2,
                pane_sequence: 3,
                session_start_source: None,
            },
            &grant,
        )
        .unwrap());
    assert!(registry
        .list_codex_session_reports("alice")
        .unwrap()
        .remove(0)
        .checkout
        .is_none());

    assert!(!gateway
        .store_codex_git_context(
            &CodexGitContext {
                session_id: Uuid::new_v4(),
                cwd: event.cwd,
                tmux_session: event.tmux_session,
                pane_id: event.pane_id,
                pane_generation: event.pane_generation,
                repository_root: None,
                repository_url: None,
                git_branch: None,
                error: None,
            },
            &grant,
        )
        .unwrap());
}

#[test]
fn new_codex_session_in_a_pane_ignores_delayed_events_from_the_previous_session() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
    let world_id = insert_world(&registry);
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path,
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();
    let mut grant = test_grant();
    grant.world_id = world_id.to_string();
    let previous_session_id = Uuid::new_v4();
    let mut event = CodexSessionEvent {
        session_id: previous_session_id,
        cwd: "/home/wt/project".into(),
        tmux_session: "wt-host".into(),
        pane_id: "%3".into(),
        kind: CodexSessionEventKind::UserPromptSubmit,
        pane_generation: 1,
        pane_sequence: 1,
        session_start_source: None,
    };
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let replacement_session_id = Uuid::new_v4();
    event.session_id = replacement_session_id;
    event.kind = CodexSessionEventKind::SessionStart;
    event.pane_generation = 2;
    event.pane_sequence = 2;
    event.session_start_source = Some(CodexSessionStartSource {
        kind: CodexSessionStartSourceKind::Clear,
        raw: "clear".into(),
    });
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == previous_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Inactive
    );
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == replacement_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Unknown
    );

    event.session_id = previous_session_id;
    event.kind = CodexSessionEventKind::UserPromptSubmit;
    event.pane_generation = 1;
    event.pane_sequence = 3;
    event.session_start_source = None;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    event.kind = CodexSessionEventKind::Stop;
    event.pane_sequence = 4;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    event.kind = CodexSessionEventKind::SessionEnd;
    event.pane_sequence = 5;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == previous_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Inactive
    );
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == replacement_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Unknown
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
    let updates = successful_push_updates(
        &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
        &response("ok refs/heads/wt/fix-login"),
        true,
    )
    .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].previous_oid, "0".repeat(40));
    assert_eq!(updates[0].new_oid, "a".repeat(40));
    assert_eq!(updates[0].reference, "refs/heads/wt/fix-login");
    insta::assert_snapshot!(
        service::push_result_message(
            Some((ProviderKind::GitHub, "acme/widget")),
            &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
            &response("ok refs/heads/wt/fix-login"),
            true,
        )
        .unwrap(),
        @r###"
    Published branch `wt/fix-login`.
    Inspect its open MR with:
      wtg tools '{"command":{"action":"show_mr_for_branch","branch":"wt/fix-login"},"target":{"provider":"github","repository":"acme/widget"}}'
    If that reports no open MR, run `wtg tools --help` and open one with an explicit base.
    Inspect CI with:
      wtg tools '{"command":{"action":"list_ci","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"target":{"provider":"github","repository":"acme/widget"}}'
    "###
    );
    let updates = successful_push_updates(
        &command(&"0".repeat(40), "refs/heads/wt/fix-login"),
        &response("ok refs/heads/wt/fix-login"),
        true,
    )
    .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].new_oid, "0".repeat(40));
    assert!(successful_push_updates(
        &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
        &response("ng refs/heads/wt/fix-login protected branch"),
        true,
    )
    .unwrap()
    .is_empty());
    insta::assert_snapshot!(
        "push_rejected",
        service::push_rejection_message(&PushViolation::Unauthorized {
            reference: "refs/heads/fix-login".to_owned()
        })
    );
}

fn test_grant() -> GrantRecord {
    GrantRecord {
        id: "id".to_owned(),
        token: "token".to_owned(),
        world_id: "world".to_owned(),
        revoked: false,
    }
}

fn insert_world(registry: &wt_workload_registry::Registry) -> Uuid {
    let id = Uuid::new_v4();
    registry
        .transaction::<_, wt_workload_registry::RegistryError>(|connection| {
            diesel::insert_into(worlds::table)
                .values((
                    worlds::id.eq(id.to_string()),
                    worlds::backend_id.eq(format!("wt-{}", id.simple())),
                    worlds::disk_id.eq(Uuid::new_v4().to_string()),
                    worlds::vcpus.eq(1_i64),
                    worlds::memory_mib.eq(1024_i64),
                    worlds::disk_gib.eq(10_i64),
                    worlds::disk_reserved_gib.eq(10_i64),
                    worlds::compute_reserved.eq(true),
                    worlds::owner.eq("alice"),
                    worlds::name.eq("checkout"),
                    worlds::status.eq("running"),
                    worlds::setup_fingerprint.eq("fingerprint"),
                    worlds::ssh_host_keys.eq("[]"),
                ))
                .execute(connection)?;
            Ok(())
        })
        .unwrap();
    id
}
