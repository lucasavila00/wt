use super::{ControlState, PaneCardIdentity};

impl ControlState {
    pub(super) fn visible_pane_identities(&self) -> Vec<PaneCardIdentity> {
        self.panes
            .iter()
            .map(|card| card.identity.clone())
            .collect()
    }

    pub(super) fn visible_pane_len(&self) -> usize {
        self.panes.len()
    }

    pub(super) fn select_first_visible_pane(&mut self) {
        let identities = self.visible_pane_identities();
        if !self
            .selected
            .as_ref()
            .is_some_and(|selected| identities.contains(selected))
        {
            self.selected = identities.first().cloned();
        }
    }
}
