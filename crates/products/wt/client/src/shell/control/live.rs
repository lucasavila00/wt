use super::{Activity, CodexCard, CodexCardIdentity, ControlState};

impl ControlState {
    pub(in super::super) fn live_codex(&self) -> Vec<&CodexCard> {
        self.codex
            .iter()
            .filter(|card| card.open_target().is_some())
            .collect()
    }

    pub(super) fn visible_codex_identities(&self) -> Vec<CodexCardIdentity> {
        if self.activity == Activity::Live {
            self.live_codex()
                .into_iter()
                .map(|card| card.identity.clone())
                .collect()
        } else {
            self.codex
                .iter()
                .map(|card| card.identity.clone())
                .collect()
        }
    }

    pub(super) fn visible_codex_len(&self) -> usize {
        if self.activity == Activity::Live {
            self.live_codex().len()
        } else {
            self.codex.len()
        }
    }

    pub(super) fn select_first_visible_codex(&mut self) {
        let identities = self.visible_codex_identities();
        if !self
            .selected
            .as_ref()
            .is_some_and(|selected| identities.contains(selected))
        {
            self.selected = identities.first().cloned();
        }
    }
}
