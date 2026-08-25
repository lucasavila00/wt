use super::*;
use diesel::prelude::*;
use wt_git_smart_protocol::validate_push;
use wt_workload_registry::schema::worlds;

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
    let policy = WritePolicy::new("refs/heads/wt/", []).unwrap();
    assert!(validate_push(&command("refs/heads/wt/fix"), &policy).is_ok());
    assert!(validate_push(&command("refs/heads/fix"), &policy).is_err());
    assert!(validate_push(&command("refs/tags/v1"), &policy).is_err());
}

#[test]
fn parses_supported_sources_without_shell_syntax() {
    let source = parse_source("git@example.test:group/repo.git").unwrap();
    assert_eq!(source.host, "example.test");
    assert_eq!(source.path, "group/repo.git");
    assert!(parse_source("git@example.test:group/repo;touch-pwned").is_err());
    assert!(parse_source("git@example.test:../repo.git").is_err());
}

#[test]
fn checkout_repository_matching_normalizes_remote_hosts() {
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

    for remote in [
        "git@GITHUB.COM:acme/project.git",
        "https://GITHUB.COM/acme/project.git",
    ] {
        assert_eq!(
            gateway.resolve_checkout_repository(Some(remote)),
            Some(service::RepositoryTarget {
                provider_host: "github.com".into(),
                repository: "acme/project".into(),
            }),
            "{remote}"
        );
    }
}

#[test]
fn repository_state_is_owner_scoped_and_honors_git_cursors() {
    let temp = tempfile::tempdir().unwrap();
    let registry = wt_workload_registry::Registry::open(&temp.path().join("instances.db")).unwrap();
    let alice = insert_world(&registry, "alice", "checkout");
    let bob = insert_world(&registry, "bob", "private-checkout");
    let session_id = Uuid::new_v4();
    registry
        .upsert_codex_session_report(wt_workload_registry::CodexSessionReportInput {
            world_id: bob.into(),
            session_id,
            cwd: "/home/wt/private",
            tmux_session: "wt-host",
            pane_id: "%1",
            state: Some(wt_workload_registry::CodexSessionState::Working),
            is_compacting: Some(false),
            pane_generation: 1,
            pane_sequence: 1,
            session_start_source: None,
        })
        .unwrap();
    registry
        .update_codex_session_git_context(wt_workload_registry::CodexSessionGitContextInput {
            world_id: bob.into(),
            session_id,
            cwd: "/home/wt/private",
            tmux_session: "wt-host",
            pane_id: "%1",
            pane_generation: 1,
            repository_root: Some("/home/wt/private"),
            repository_url: Some("git@github.com:private/repository.git"),
            git_branch: Some("main"),
            repository_target: Some(wt_workload_registry::RepositoryTargetInput {
                provider_host: "github.com",
                repository: "private/repository",
            }),
            error: None,
        })
        .unwrap();
    assert!(registry
        .repository_git_state("alice", "github.com", "private/repository", None, None)
        .unwrap()
        .is_none());

    for branch in ["wt/first", "wt/second"] {
        registry
            .insert_git_activity(wt_workload_registry::GitActivityInput {
                world_id: alice.into(),
                kind: wt_workload_registry::GitActivityKind::BranchUpdate,
                provider_host: "github.com",
                repository: "acme/project",
                git_service: Some("receive-pack"),
                branch: Some(branch),
                previous_oid: Some("0"),
                new_oid: Some("1"),
            })
            .unwrap();
    }
    let first_page = registry
        .repository_git_state("alice", "github.com", "acme/project", None, None)
        .unwrap()
        .unwrap();
    assert_eq!(first_page.git_activity.len(), 2);
    let second_page = registry
        .repository_git_state(
            "alice",
            "github.com",
            "acme/project",
            Some(first_page.git_activity[0].id),
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(second_page.git_activity.len(), 1);
    assert_eq!(
        second_page.git_activity[0].branch.as_deref(),
        Some("wt/first")
    );
}

fn insert_world(registry: &wt_workload_registry::Registry, owner: &str, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    registry
        .transaction::<_, wt_workload_registry::RegistryError>(|connection| {
            diesel::insert_into(worlds::table)
                .values((
                    worlds::world_id.eq(id.to_string()),
                    worlds::vcpus.eq(1_i64),
                    worlds::memory_mib.eq(1024_i64),
                    worlds::disk_gib.eq(10_i64),
                    worlds::disk_reserved_gib.eq(10_i64),
                    worlds::compute_reserved.eq(true),
                    worlds::owner.eq(owner),
                    worlds::name.eq(name),
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
