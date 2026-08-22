use ratatui::layout::Rect;

use super::control::{CodexOpenTarget, ControlAction, ControlState};

const WIDTH: u16 = 52;
const HEIGHT: u16 = 5;

pub(super) fn area(outer: Rect) -> Rect {
    let width = WIDTH.min(outer.width.saturating_sub(2));
    let height = HEIGHT.min(outer.height.saturating_sub(2));
    Rect::new(
        outer.right().saturating_sub(width).saturating_sub(1),
        outer.bottom().saturating_sub(height).saturating_sub(1),
        width,
        height,
    )
}

pub(super) fn actions(outer: Rect) -> (Rect, Rect) {
    let toast = area(outer);
    let retry = Rect::new(
        toast.x.saturating_add(1),
        toast.bottom().saturating_sub(2),
        toast.width.saturating_sub(2),
        1,
    );
    let dismiss = Rect::new(toast.right().saturating_sub(2), toast.y, 1, 1);
    (retry, dismiss)
}

impl ControlState {
    pub(super) fn open_failed(&self) -> bool {
        self.open_failure.is_some()
    }

    pub(super) fn context_failure(&self) -> Option<&[String]> {
        self.codex_refresh().failures()
    }

    pub(super) fn set_context_failures(&mut self, contexts: Vec<String>) {
        self.codex_refresh_mut().set_failures(contexts);
    }

    pub(super) fn finish_open(&mut self, target: &CodexOpenTarget, failed: bool) -> bool {
        if self.opening.as_ref() != Some(&target.identity) {
            return false;
        }
        self.opening = None;
        self.open_failure = failed.then(|| target.clone());
        true
    }

    pub(super) fn retry_open(&mut self) -> Option<ControlAction> {
        let target = self.open_failure.take()?;
        self.opening = Some(target.identity.clone());
        Some(ControlAction::OpenCodex(Box::new(target)))
    }
}
