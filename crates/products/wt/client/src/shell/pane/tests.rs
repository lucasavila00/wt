use super::*;

fn observation(world: &ShellWorld) -> PaneObservation {
    PaneObservation {
        world_id: world.identity.world_id,
        world_name: world.world_name.clone(),
        tmux_session: "wt-host".into(),
        pane_id: "%1".into(),
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
        vec![observation(&world)],
        std::slice::from_ref(&world),
    )
    .unwrap();

    assert_eq!(cards.len(), 1);
    assert!(cards[0].changed_recently());
}

#[test]
fn rejects_invalid_pane_observations() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut invalid_pane = observation(&world);
    invalid_pane.pane_id = "%not-a-number".into();
    insta::assert_snapshot!(
        validate_context("ars", vec![invalid_pane], &[world]).unwrap_err(),
        @"context ars: failed invariant pane_id is % plus 1-16 ASCII digits; value \"%not-a-number\""
    );
}

#[test]
fn marks_an_unrefreshed_observation_stale() {
    let world = ShellWorld::test("ars.dev", 1);
    let mut stale = observation(&world);
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
