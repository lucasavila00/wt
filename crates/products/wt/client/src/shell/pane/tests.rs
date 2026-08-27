use super::*;

fn observation(world: &ShellWorld, created_at_unix_ms: i64) -> PaneObservation {
    PaneObservation {
        world_id: world.identity.world_id,
        world_name: world.world_name.clone(),
        created_at_unix_ms,
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
        cwd: "/home/wt".into(),
        git_branch: None,
        changed_at_unix_ms: now_unix_ms(),
        observed_at_unix_ms: now_unix_ms(),
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

#[test]
fn creates_pane_cards_directly_from_observations() {
    let world = ShellWorld::test("ars.dev", 1);
    let cards = validate_context(
        "ars",
        vec![observation(&world, 1)],
        std::slice::from_ref(&world),
    )
    .unwrap();

    assert_eq!(cards.len(), 1);
    assert!(cards[0].changed_recently());
}

#[test]
fn rejects_invalid_pane_observations() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut invalid_pane = observation(&world, 1);
    invalid_pane.pane_id = "%not-a-number".into();
    insta::assert_snapshot!(
        validate_context("ars", vec![invalid_pane], &[world]).unwrap_err(),
        @"context ars: failed invariant pane_id is % plus 1-16 ASCII digits; value \"%not-a-number\""
    );
}

#[test]
fn marks_an_unrefreshed_observation_stale() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut stale = observation(&world, 1);
    stale.changed_at_unix_ms = 0;
    stale.observed_at_unix_ms = 0;
    let cards = validate_context("ars", vec![stale], &[world]).unwrap();

    assert!(cards[0].is_stale());
    assert!(!cards[0].changed_recently());
}

#[test]
fn query_failures_preserve_the_error_instead_of_creating_cards() {
    let result = cards(
        vec![PaneContextSnapshot::Failure {
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

#[test]
fn groups_paused_panes_before_active_panes_in_creation_order() {
    let active_first = ShellWorld::test("active-first", 1);
    let paused_first = ShellWorld::test("paused-first", 2);
    let paused_second = ShellWorld::test("paused-second", 3);
    let active_second = ShellWorld::test("active-second", 4);
    let expected = [
        paused_first.identity.world_id,
        paused_second.identity.world_id,
        active_first.identity.world_id,
        active_second.identity.world_id,
    ];
    let now = now_unix_ms();
    let paused_at = now - 60_000;
    let panes = vec![
        observation(&active_second, 40),
        PaneObservation {
            changed_at_unix_ms: paused_at,
            ..observation(&paused_second, 30)
        },
        observation(&active_first, 10),
        PaneObservation {
            changed_at_unix_ms: paused_at,
            ..observation(&paused_first, 20)
        },
    ];

    let result = cards(
        vec![PaneContextSnapshot::Panes {
            context: "local".into(),
            panes,
        }],
        &[active_first, paused_first, paused_second, active_second],
    );

    let world_ids = result
        .cards
        .iter()
        .map(|card| card.world_id().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(world_ids, expected);
}

#[test]
fn retains_the_observed_cwd_and_git_branch() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut pane = observation(&world, 1);
    pane.cwd = "/home/wt/wt".into();
    pane.git_branch = Some("wt/live-pane-cwd".into());

    let cards = validate_context("ars", vec![pane], &[world]).unwrap();

    assert_eq!(
        cards[0].location().as_deref(),
        Some("/home/wt/wt · wt/live-pane-cwd")
    );
}
