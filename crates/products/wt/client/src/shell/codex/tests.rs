use super::*;
use wt_control_protocol::{ByobuTarget, CodexSessionObservation, CodexSessionState, WorldName};

fn session(world: &ShellWorld, cwd: &str) -> CodexSession {
    CodexSession {
        session_id: Uuid::from_u128(10),
        title: Some("Improve session cards".into()),
        latest_user_message: Some("Make the cards taller and show the latest request".into()),
        latest_user_message_at_unix_ms: Some(9),
        latest_agent_message: None,
        latest_agent_message_at_unix_ms: None,
        created_at_unix_ms: None,
        rollout_updated_at_unix_ms: Some(10),
        cwd: None,
        model: None,
        cli_version: None,
        turn_count: 3,
        command_count: 4,
        file_change_count: 2,
        input_tokens: 1_000,
        cached_input_tokens: 800,
        output_tokens: 200,
        reasoning_output_tokens: 50,
        observations: vec![CodexSessionObservation {
            world_id: world.identity.world_id,
            world_name: world.world_name.clone(),
            cwd: cwd.into(),
            repository_root: Some("/home/wt/project".into()),
            repository_url: Some("git@github.com:acme/project.git".into()),
            git_branch: Some("wt/cards".into()),
            git_context_checked_at_unix_ms: None,
            git_context_error: None,
            state: CodexSessionState::NeedsAttention,
            is_compacting: false,
            session_start_source: None,
            target: ByobuTarget {
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
            },
            received_at_unix_ms: 20,
        }],
    }
}

#[test]
fn validates_complete_context_before_creating_cards() {
    let world = ShellWorld::test("ars.dev", 1);
    let cards = validate_context(
        "ars",
        vec![session(&world, "/home/wt/project")],
        std::slice::from_ref(&world),
    )
    .unwrap();
    assert_eq!(cards.len(), 1);
    assert!(cards[0].open_target().is_some());

    insta::assert_snapshot!(
        validate_context("ars", vec![session(&world, "relative")], &[world]).unwrap_err(),
        @"context ars: failed invariant absolute control-free cwd; value relative"
    );
}

#[test]
fn rejects_world_name_and_tmux_mismatches() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut wrong_name = session(&world, "/home/wt/project");
    wrong_name.observations[0].world_name = WorldName::parse("other").unwrap();
    insta::assert_snapshot!(
        validate_context("ars", vec![wrong_name], std::slice::from_ref(&world)).unwrap_err(),
        @"context ars: failed invariant world_name matches inventory world_id; value other"
    );

    let mut wrong_tmux = session(&world, "/home/wt/project");
    wrong_tmux.observations[0].target.tmux_session = "other".into();
    insta::assert_snapshot!(
        validate_context("ars", vec![wrong_tmux], &[world]).unwrap_err(),
        @"context ars: failed invariant tmux_session is wt-host; value other"
    );
}

#[test]
fn rejects_duplicate_sessions_and_negative_timestamps() {
    let world = ShellWorld::test("ars.dev", 1);
    let valid = session(&world, "/home/wt/project");
    insta::assert_snapshot!(
        validate_context("ars", vec![valid.clone(), valid.clone()], std::slice::from_ref(&world))
            .unwrap_err(),
        @"context ars: failed invariant unique session_id; value 00000000-0000-0000-0000-00000000000a"
    );

    let mut negative = valid;
    negative.observations[0].received_at_unix_ms = -1;
    insta::assert_snapshot!(
        validate_context("ars", vec![negative], &[world]).unwrap_err(),
        @"context ars: failed invariant nonnegative observation timestamp; value 00000000-0000-0000-0000-00000000000a"
    );
}

#[test]
fn query_failures_preserve_the_error_instead_of_creating_cards() {
    let result = cards(
        vec![CodexContextSnapshot::Failure {
            message: "context ars could not be queried: server rejected the request".into(),
        }],
        &[],
    );

    assert!(result.cards.is_empty());
    assert_eq!(
        result.failures,
        ["context ars could not be queried: server rejected the request"]
    );
}
