use super::Activity;
use ratatui::layout::Rect;
use ratatui::text::Span;

pub(in crate::shell) const HELP_CONTROL: &str = "[ Help (2 / F2) ]";

#[derive(Debug, Default)]
pub(in crate::shell) struct Help {
    open: bool,
}

impl Help {
    pub(in crate::shell) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::shell) fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub(in crate::shell) fn close(&mut self) {
        self.open = false;
    }

    pub(in crate::shell) fn rows(
        &self,
        activity: Activity,
        has_worlds: bool,
    ) -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![
            ("Tab", "Next activity"),
            ("Arrows / wheel", "Select card"),
            ("Enter / click", "Open selected card"),
            ("1 / F1", "Open command palette"),
            ("2 / F2", "Toggle help"),
        ];
        if has_worlds {
            rows.push(("F5", "Open active world"));
        }
        rows.push(("F6", "Close WT"));
        if activity == Activity::Worlds && !has_worlds {
            rows.retain(|(keys, _)| !matches!(*keys, "Arrows / wheel" | "Enter / click"));
        }
        rows
    }
}

pub(in crate::shell) fn help_control_area(footer: Rect) -> Rect {
    let width = u16::try_from(Span::raw(HELP_CONTROL).width()).unwrap_or(u16::MAX);
    Rect::new(
        footer.right().saturating_sub(width.min(footer.width)),
        footer.y,
        width.min(footer.width),
        1.min(footer.height),
    )
}
