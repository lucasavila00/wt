use super::codex::FocusWorker;
use super::control::{Activity, CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget};
use super::model::{Mode, ShellModel};
use super::session::SessionSet;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
#[cfg(test)]
use uuid::Uuid;
use wt_control_protocol::CodexSessionState;
use wt_control_protocol::WorldId;

const STUCK_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Policy {
    Clear,
    Pause,
    Detect,
}

pub(super) fn policy(mode: Mode, activity: Activity, action_running: bool) -> Policy {
    if action_running {
        Policy::Clear
    } else if mode == Mode::Control && activity == Activity::Live {
        Policy::Pause
    } else {
        Policy::Detect
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Pending,
    Focused,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FocusState {
    status: Status,
    stream_token: Option<u64>,
}

impl FocusState {
    fn matches_stream(self, stream_token: u64) -> bool {
        self.stream_token == Some(stream_token)
    }
}

pub(super) struct CodexScreenTracker {
    states: BTreeMap<CodexCardIdentity, FocusState>,
    changes: BTreeMap<CodexCardIdentity, ScreenChangeDetection>,
    stuck_after: Duration,
    paused: bool,
}

struct ScreenChangeDetection {
    screen: ((u16, u16), String),
    lifecycle: (Option<i64>, CodexSessionState),
    stream_token: u64,
    quiet_since: Instant,
    stuck: bool,
}

impl CodexScreenTracker {
    pub(super) fn new(stuck_after: Duration) -> Self {
        Self {
            states: BTreeMap::new(),
            changes: BTreeMap::new(),
            stuck_after,
            paused: false,
        }
    }

    pub(super) fn sync_focus(
        &mut self,
        model: &ShellModel,
        sessions: &SessionSet,
        worker: &FocusWorker,
    ) {
        let targets = model
            .control()
            .codex()
            .iter()
            .filter_map(CodexCard::open_target)
            .collect::<Vec<_>>();
        let counts = world_counts(&targets);
        let unique = targets
            .iter()
            .filter(|target| counts.get(&world_key(target)).copied() == Some(1))
            .map(|target| target.identity.clone())
            .collect::<BTreeSet<_>>();
        self.states.retain(|identity, _| unique.contains(identity));

        for target in targets {
            if counts.get(&world_key(&target)).copied() != Some(1) {
                continue;
            }
            let Some((index, alias)) = model.focus_route(&target) else {
                self.states.insert(
                    target.identity,
                    FocusState {
                        status: Status::Failed,
                        stream_token: None,
                    },
                );
                continue;
            };
            if !sessions.is_open(index) {
                self.states.insert(
                    target.identity,
                    FocusState {
                        status: Status::Failed,
                        stream_token: None,
                    },
                );
                continue;
            }
            let stream_token = sessions.token(index);
            if self
                .states
                .get(&target.identity)
                .is_some_and(|state| state.matches_stream(stream_token))
            {
                continue;
            }
            self.states.insert(
                target.identity.clone(),
                FocusState {
                    status: Status::Pending,
                    stream_token: Some(stream_token),
                },
            );
            worker.start_live(
                target,
                alias.to_owned(),
                sessions.control_path(index).to_owned(),
            );
        }
    }

    pub(super) fn detect_screen_changes(
        &mut self,
        model: &ShellModel,
        sessions: &SessionSet,
    ) -> bool {
        let resumed = self.resume();
        self.update_change_detection(model, sessions, Instant::now()) || resumed
    }

    pub(super) fn complete(&mut self, target: &CodexOpenTarget, focused: bool) {
        if let Some(state) = self.states.get_mut(&target.identity) {
            state.status = if focused {
                Status::Focused
            } else {
                Status::Failed
            };
        }
    }

    pub(super) fn clear(&mut self) {
        self.states.clear();
        self.changes.clear();
        self.paused = false;
    }

    pub(super) fn pause_change_detection(
        &mut self,
        model: &ShellModel,
        sessions: &SessionSet,
    ) -> bool {
        let counts = active_world_counts(model);
        let eligible = model
            .control()
            .codex()
            .iter()
            .filter_map(|card| {
                let eligible = eligible_detection(card, model, sessions, &counts)?;
                self.states
                    .get(&card.identity)
                    .is_some_and(|focus| {
                        focus.status != Status::Failed
                            && focus.matches_stream(eligible.stream_token)
                    })
                    .then(|| {
                        (
                            card.identity.clone(),
                            (eligible.lifecycle, eligible.stream_token),
                        )
                    })
            })
            .collect::<BTreeMap<_, _>>();
        self.pause_for(&eligible)
    }

    fn pause_for(
        &mut self,
        eligible: &BTreeMap<CodexCardIdentity, ((Option<i64>, CodexSessionState), u64)>,
    ) -> bool {
        self.paused = true;
        let mut changed = false;
        self.changes.retain(|identity, detection| {
            let retained =
                eligible.get(identity) == Some(&(detection.lifecycle, detection.stream_token));
            changed |= !retained && detection.stuck;
            retained
        });
        changed
    }

    fn resume(&mut self) -> bool {
        let resumed = self.paused;
        if resumed {
            self.changes.clear();
            self.paused = false;
        }
        resumed
    }

    pub(super) fn is_stuck(&self, card: &CodexCard) -> bool {
        let Some(detection) = self.changes.get(&card.identity) else {
            return false;
        };
        detection.stuck
            && self.states.get(&card.identity).is_some_and(|focus| {
                focus.status == Status::Focused && focus.matches_stream(detection.stream_token)
            })
    }

    pub(super) fn warning(&self, card: &CodexCard, cards: &[CodexCard]) -> Option<&'static str> {
        let CodexCardKind::Observation { world_id, .. } = &card.kind else {
            return None;
        };
        let count = cards
            .iter()
            .filter(|candidate| {
                matches!(
                    &candidate.kind,
                    CodexCardKind::Observation {
                        world_id: candidate_world,
                        state,
                        ..
                    } if candidate.context == card.context
                        && candidate_world == world_id
                        && *state != wt_control_protocol::CodexSessionState::Inactive
                )
            })
            .count();
        if count > 1 {
            return Some("Multiple Codex sessions in this world; open one to choose its pane");
        }
        match self.states.get(&card.identity).map(|state| state.status) {
            Some(Status::Pending) => Some("Focusing world on this Codex session…"),
            Some(Status::Failed) => Some("World not focused on this Codex session"),
            Some(Status::Focused) | None => None,
        }
    }

    fn update_change_detection(
        &mut self,
        model: &ShellModel,
        sessions: &SessionSet,
        now: Instant,
    ) -> bool {
        let mut eligible = BTreeSet::new();
        let mut changed = false;
        let counts = active_world_counts(model);
        for card in model.control().codex() {
            let Some(detection) = eligible_detection(card, model, sessions, &counts) else {
                continue;
            };
            eligible.insert(card.identity.clone());
            let screen = sessions.screen(detection.index);
            let screen = (screen.size(), screen.contents());
            match self.changes.get_mut(&card.identity) {
                Some(session) => {
                    changed |= session.update(
                        screen,
                        detection.lifecycle,
                        detection.stream_token,
                        now,
                        self.stuck_after,
                    );
                }
                None => {
                    self.changes.insert(
                        card.identity.clone(),
                        ScreenChangeDetection::new(
                            screen,
                            detection.lifecycle,
                            detection.stream_token,
                            now,
                        ),
                    );
                }
            }
        }
        self.changes.retain(|identity, session| {
            let retained = eligible.contains(identity);
            changed |= !retained && session.stuck;
            retained
        });
        changed
    }
}

struct EligibleDetection {
    index: usize,
    lifecycle: (Option<i64>, CodexSessionState),
    stream_token: u64,
}

fn eligible_detection(
    card: &CodexCard,
    model: &ShellModel,
    sessions: &SessionSet,
    counts: &BTreeMap<(String, WorldId), usize>,
) -> Option<EligibleDetection> {
    let CodexCardKind::Observation {
        world_id,
        state,
        is_compacting,
        ..
    } = &card.kind
    else {
        return None;
    };
    if *state != CodexSessionState::Working || *is_compacting {
        return None;
    }
    let target = card.open_target()?;
    if counts.get(&world_key(&target)).copied() != Some(1) {
        return None;
    }
    let index = model.worlds().iter().position(|world| {
        world.identity.context == card.context && world.identity.world_id == *world_id
    })?;
    if !sessions.is_open(index) {
        return None;
    }
    Some(EligibleDetection {
        index,
        lifecycle: (card.timestamp, *state),
        stream_token: sessions.token(index),
    })
}

fn active_world_counts(model: &ShellModel) -> BTreeMap<(String, WorldId), usize> {
    world_counts(
        &model
            .control()
            .codex()
            .iter()
            .filter_map(CodexCard::open_target)
            .collect::<Vec<_>>(),
    )
}

impl ScreenChangeDetection {
    fn new(
        screen: ((u16, u16), String),
        lifecycle: (Option<i64>, CodexSessionState),
        stream_token: u64,
        now: Instant,
    ) -> Self {
        Self {
            screen,
            lifecycle,
            stream_token,
            quiet_since: now,
            stuck: false,
        }
    }

    fn update(
        &mut self,
        screen: ((u16, u16), String),
        lifecycle: (Option<i64>, CodexSessionState),
        stream_token: u64,
        now: Instant,
        threshold: Duration,
    ) -> bool {
        let was_stuck = self.stuck;
        if self.screen != screen || self.lifecycle != lifecycle || self.stream_token != stream_token
        {
            self.screen = screen;
            self.lifecycle = lifecycle;
            self.stream_token = stream_token;
            self.quiet_since = now;
            self.stuck = false;
        } else if now.saturating_duration_since(self.quiet_since) >= threshold {
            self.stuck = true;
        }
        self.stuck != was_stuck
    }
}

impl Default for CodexScreenTracker {
    fn default() -> Self {
        Self::new(STUCK_AFTER)
    }
}

fn world_counts(targets: &[CodexOpenTarget]) -> BTreeMap<(String, WorldId), usize> {
    let mut counts = BTreeMap::new();
    for target in targets {
        *counts.entry(world_key(target)).or_default() += 1;
    }
    counts
}

fn world_key(target: &CodexOpenTarget) -> (String, WorldId) {
    (target.context.clone(), target.world_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt_control_protocol::{ByobuTarget, CodexSessionState};

    fn card(session_id: u128) -> CodexCard {
        let session_id = Uuid::from_u128(session_id);
        CodexCard {
            identity: CodexCardIdentity::Observation {
                context: "local".into(),
                session_id,
                world_id: Uuid::from_u128(1).into(),
                tmux_session: "wt-host".into(),
                pane_id: format!("%{session_id}"),
            },
            context: "local".into(),
            session_id: Some(session_id),
            timestamp: Some(1),
            latest_user_message: None,
            kind: CodexCardKind::Observation {
                world_id: Uuid::from_u128(1).into(),
                world_name: "world".into(),
                cwd: "/home/wt".into(),
                repository_root: None,
                repository_url: None,
                git_branch: None,
                git_context_health: None,
                state: CodexSessionState::Working,
                is_compacting: false,
                session_start_source: None,
                target: ByobuTarget {
                    tmux_session: "wt-host".into(),
                    pane_id: format!("%{session_id}"),
                },
            },
        }
    }

    #[test]
    fn multiple_sessions_in_one_world_are_explicitly_ambiguous() {
        let cards = vec![card(1), card(2)];
        assert_eq!(
            CodexScreenTracker::default().warning(&cards[0], &cards),
            Some("Multiple Codex sessions in this world; open one to choose its pane")
        );
    }

    #[test]
    fn failed_unique_focus_has_an_explicit_warning() {
        let cards = vec![card(1)];
        let mut focus = CodexScreenTracker::default();
        focus.states.insert(
            cards[0].identity.clone(),
            FocusState {
                status: Status::Failed,
                stream_token: Some(1),
            },
        );
        assert_eq!(
            focus.warning(&cards[0], &cards),
            Some("World not focused on this Codex session")
        );
    }

    #[test]
    fn unchanged_screen_becomes_stuck_at_the_threshold_and_resets_on_change() {
        let started = Instant::now();
        let lifecycle = (Some(1), CodexSessionState::Working);
        let screen = ((16, 80), "screen".into());
        let mut detection = ScreenChangeDetection::new(screen.clone(), lifecycle, 7, started);

        assert!(!detection.update(
            screen.clone(),
            lifecycle,
            7,
            started + Duration::from_secs(29),
            Duration::from_secs(30)
        ));
        assert!(detection.update(
            screen,
            lifecycle,
            7,
            started + Duration::from_secs(30),
            Duration::from_secs(30)
        ));
        assert!(detection.stuck);
        assert!(detection.update(
            ((16, 80), "changed".into()),
            lifecycle,
            7,
            started + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
        assert!(!detection.stuck);
    }

    #[test]
    fn resizing_resets_detection_even_when_its_text_is_unchanged() {
        let started = Instant::now();
        let lifecycle = (Some(1), CodexSessionState::Working);
        let mut detection =
            ScreenChangeDetection::new(((16, 80), "screen".into()), lifecycle, 7, started);
        assert!(detection.update(
            ((16, 80), "screen".into()),
            lifecycle,
            7,
            started + Duration::from_secs(30),
            Duration::from_secs(30)
        ));

        assert!(detection.update(
            ((20, 100), "screen".into()),
            lifecycle,
            7,
            started + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
        assert!(!detection.stuck);
    }

    #[test]
    fn replacing_the_playback_stream_resets_detection() {
        let started = Instant::now();
        let lifecycle = (Some(1), CodexSessionState::Working);
        let screen = ((16, 80), "screen".into());
        let mut detection = ScreenChangeDetection::new(screen.clone(), lifecycle, 7, started);
        assert!(detection.update(
            screen.clone(),
            lifecycle,
            7,
            started + Duration::from_secs(30),
            Duration::from_secs(30)
        ));

        assert!(detection.update(
            screen,
            lifecycle,
            8,
            started + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
        assert!(!detection.stuck);
    }

    #[test]
    fn focus_is_valid_only_for_the_stream_that_was_checked() {
        let focus = FocusState {
            status: Status::Focused,
            stream_token: Some(7),
        };

        assert!(focus.matches_stream(7));
        assert!(!focus.matches_stream(8));
    }

    #[test]
    fn pausing_preserves_the_hint_until_observation_resumes() {
        let card = card(1);
        let mut focus = CodexScreenTracker::default();
        focus.states.insert(
            card.identity.clone(),
            FocusState {
                status: Status::Focused,
                stream_token: Some(7),
            },
        );
        focus.changes.insert(
            card.identity.clone(),
            ScreenChangeDetection {
                screen: ((16, 80), "screen".into()),
                lifecycle: (Some(1), CodexSessionState::Working),
                stream_token: 7,
                quiet_since: Instant::now(),
                stuck: true,
            },
        );

        focus.pause_for(&BTreeMap::from([(
            card.identity.clone(),
            ((Some(1), CodexSessionState::Working), 7),
        )]));
        assert!(focus.is_stuck(&card));
        assert!(focus.resume());
        assert!(!focus.is_stuck(&card));
    }

    #[test]
    fn paused_detection_drops_a_stale_lifecycle_hint() {
        let card = card(1);
        let mut focus = CodexScreenTracker::default();
        focus.changes.insert(
            card.identity.clone(),
            ScreenChangeDetection {
                screen: ((16, 80), "screen".into()),
                lifecycle: (Some(1), CodexSessionState::Working),
                stream_token: 7,
                quiet_since: Instant::now(),
                stuck: true,
            },
        );

        assert!(focus.pause_for(&BTreeMap::from([(
            card.identity.clone(),
            ((Some(2), CodexSessionState::NeedsAttention), 7),
        )])));
        assert!(!focus.is_stuck(&card));
    }

    #[test]
    fn paused_detection_drops_a_hint_without_verified_live_focus() {
        let card = card(1);
        let mut tracker = CodexScreenTracker::default();
        tracker.changes.insert(
            card.identity.clone(),
            ScreenChangeDetection {
                screen: ((16, 80), "screen".into()),
                lifecycle: (Some(1), CodexSessionState::Working),
                stream_token: 7,
                quiet_since: Instant::now(),
                stuck: true,
            },
        );

        assert!(tracker.pause_for(&BTreeMap::new()));
        assert!(!tracker.is_stuck(&card));
    }

    #[test]
    fn pending_live_focus_hides_then_reveals_the_preserved_hint() {
        let card = card(1);
        let target = card.open_target().unwrap();
        let mut tracker = CodexScreenTracker::default();
        tracker.states.insert(
            card.identity.clone(),
            FocusState {
                status: Status::Pending,
                stream_token: Some(7),
            },
        );
        tracker.changes.insert(
            card.identity.clone(),
            ScreenChangeDetection {
                screen: ((16, 80), "screen".into()),
                lifecycle: (Some(1), CodexSessionState::Working),
                stream_token: 7,
                quiet_since: Instant::now(),
                stuck: true,
            },
        );

        tracker.pause_for(&BTreeMap::from([(
            card.identity.clone(),
            ((Some(1), CodexSessionState::Working), 7),
        )]));
        assert!(!tracker.is_stuck(&card));
        tracker.complete(&target, true);
        assert!(tracker.is_stuck(&card));
    }
}
