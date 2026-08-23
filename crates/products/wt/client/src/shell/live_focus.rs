use super::codex::FocusWorker;
use super::control::{CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget};
use super::model::ShellModel;
use super::session::SessionSet;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_control_protocol::CodexSessionState;

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

pub(super) struct LiveFocus {
    states: BTreeMap<CodexCardIdentity, FocusState>,
    quiet: BTreeMap<CodexCardIdentity, QuietSession>,
    stuck_after: Duration,
}

struct QuietSession {
    screen: ((u16, u16), String),
    lifecycle: (Option<i64>, CodexSessionState),
    quiet_since: Instant,
    stuck: bool,
}

impl LiveFocus {
    pub(super) fn new(stuck_after: Duration) -> Self {
        Self {
            states: BTreeMap::new(),
            quiet: BTreeMap::new(),
            stuck_after,
        }
    }

    pub(super) fn sync(
        &mut self,
        model: &ShellModel,
        sessions: &SessionSet,
        worker: &FocusWorker,
    ) -> bool {
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
        self.sync_quiet(model, sessions, Instant::now())
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
        self.quiet.clear();
    }

    pub(super) fn is_stuck(&self, card: &CodexCard) -> bool {
        self.quiet
            .get(&card.identity)
            .is_some_and(|session| session.stuck)
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

    fn sync_quiet(&mut self, model: &ShellModel, sessions: &SessionSet, now: Instant) -> bool {
        let mut eligible = BTreeSet::new();
        let mut changed = false;
        for card in model.control().codex() {
            let CodexCardKind::Observation {
                world_id,
                state,
                is_compacting,
                ..
            } = &card.kind
            else {
                continue;
            };
            if *state != CodexSessionState::Working
                || *is_compacting
                || self.states.get(&card.identity).map(|state| state.status)
                    != Some(Status::Focused)
            {
                continue;
            }
            let Some(index) = model.worlds().iter().position(|world| {
                world.identity.context == card.context && world.identity.id == *world_id
            }) else {
                continue;
            };
            if !sessions.is_open(index) {
                continue;
            }
            eligible.insert(card.identity.clone());
            let screen = sessions.screen(index);
            let screen = (screen.size(), screen.contents());
            let lifecycle = (card.timestamp, *state);
            match self.quiet.get_mut(&card.identity) {
                Some(session) => {
                    changed |= session.update(screen, lifecycle, now, self.stuck_after);
                }
                None => {
                    self.quiet.insert(
                        card.identity.clone(),
                        QuietSession::new(screen, lifecycle, now),
                    );
                }
            }
        }
        self.quiet.retain(|identity, session| {
            let retained = eligible.contains(identity);
            changed |= !retained && session.stuck;
            retained
        });
        changed
    }
}

impl QuietSession {
    fn new(
        screen: ((u16, u16), String),
        lifecycle: (Option<i64>, CodexSessionState),
        now: Instant,
    ) -> Self {
        Self {
            screen,
            lifecycle,
            quiet_since: now,
            stuck: false,
        }
    }

    fn update(
        &mut self,
        screen: ((u16, u16), String),
        lifecycle: (Option<i64>, CodexSessionState),
        now: Instant,
        threshold: Duration,
    ) -> bool {
        let was_stuck = self.stuck;
        if self.screen != screen || self.lifecycle != lifecycle {
            self.screen = screen;
            self.lifecycle = lifecycle;
            self.quiet_since = now;
            self.stuck = false;
        } else if now.saturating_duration_since(self.quiet_since) >= threshold {
            self.stuck = true;
        }
        self.stuck != was_stuck
    }
}

impl Default for LiveFocus {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

fn world_counts(targets: &[CodexOpenTarget]) -> BTreeMap<(String, Uuid), usize> {
    let mut counts = BTreeMap::new();
    for target in targets {
        *counts.entry(world_key(target)).or_default() += 1;
    }
    counts
}

fn world_key(target: &CodexOpenTarget) -> (String, Uuid) {
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
                world_id: Uuid::from_u128(1),
                tmux_session: "wt-host".into(),
                pane_id: format!("%{session_id}"),
            },
            context: "local".into(),
            session_id: Some(session_id),
            timestamp: Some(1),
            latest_user_message: None,
            kind: CodexCardKind::Observation {
                world_id: Uuid::from_u128(1),
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
            LiveFocus::default().warning(&cards[0], &cards),
            Some("Multiple Codex sessions in this world; open one to choose its pane")
        );
    }

    #[test]
    fn failed_unique_focus_has_an_explicit_warning() {
        let cards = vec![card(1)];
        let mut focus = LiveFocus::default();
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
    fn quiet_session_becomes_stuck_at_the_threshold_and_resets_on_change() {
        let started = Instant::now();
        let lifecycle = (Some(1), CodexSessionState::Working);
        let screen = ((16, 80), "screen".into());
        let mut quiet = QuietSession::new(screen.clone(), lifecycle, started);

        assert!(!quiet.update(
            screen.clone(),
            lifecycle,
            started + Duration::from_secs(29),
            Duration::from_secs(30)
        ));
        assert!(quiet.update(
            screen,
            lifecycle,
            started + Duration::from_secs(30),
            Duration::from_secs(30)
        ));
        assert!(quiet.stuck);
        assert!(quiet.update(
            ((16, 80), "changed".into()),
            lifecycle,
            started + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
        assert!(!quiet.stuck);
    }

    #[test]
    fn resizing_resets_a_quiet_session_even_when_its_text_is_unchanged() {
        let started = Instant::now();
        let lifecycle = (Some(1), CodexSessionState::Working);
        let mut quiet = QuietSession::new(((16, 80), "screen".into()), lifecycle, started);
        assert!(quiet.update(
            ((16, 80), "screen".into()),
            lifecycle,
            started + Duration::from_secs(30),
            Duration::from_secs(30)
        ));

        assert!(quiet.update(
            ((20, 100), "screen".into()),
            lifecycle,
            started + Duration::from_secs(31),
            Duration::from_secs(30)
        ));
        assert!(!quiet.stuck);
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
}
