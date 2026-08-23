use super::codex::FocusWorker;
use super::control::{CodexCard, CodexCardIdentity, CodexCardKind, CodexOpenTarget};
use super::model::ShellModel;
use super::session::SessionSet;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Pending,
    Focused,
    Failed,
}

#[derive(Default)]
pub(super) struct LiveFocus {
    states: BTreeMap<CodexCardIdentity, Status>,
}

impl LiveFocus {
    pub(super) fn sync(&mut self, model: &ShellModel, sessions: &SessionSet, worker: &FocusWorker) {
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
            if counts.get(&world_key(&target)).copied() != Some(1)
                || self.states.contains_key(&target.identity)
            {
                continue;
            }
            let Some((index, alias)) = model.focus_route(&target) else {
                self.states.insert(target.identity, Status::Failed);
                continue;
            };
            if !sessions.is_open(index) {
                self.states.insert(target.identity, Status::Failed);
                continue;
            }
            self.states.insert(target.identity.clone(), Status::Pending);
            worker.start_live(
                target,
                alias.to_owned(),
                sessions.control_path(index).to_owned(),
            );
        }
    }

    pub(super) fn complete(&mut self, target: &CodexOpenTarget, focused: bool) {
        if self.states.contains_key(&target.identity) {
            self.states.insert(
                target.identity.clone(),
                if focused {
                    Status::Focused
                } else {
                    Status::Failed
                },
            );
        }
    }

    pub(super) fn clear(&mut self) {
        self.states.clear();
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
        match self.states.get(&card.identity) {
            Some(Status::Pending) => Some("Focusing world on this Codex session…"),
            Some(Status::Failed) => Some("World not focused on this Codex session"),
            Some(Status::Focused) | None => None,
        }
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
        focus
            .states
            .insert(cards[0].identity.clone(), Status::Failed);
        assert_eq!(
            focus.warning(&cards[0], &cards),
            Some("World not focused on this Codex session")
        );
    }
}
