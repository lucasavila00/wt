use super::control::CodexOpenTarget;
use super::model::{ShellWorld, WorldIdentity};
use crate::create::Input;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use std::collections::VecDeque;

pub(super) type ActionId = u64;

#[derive(Clone, Debug)]
pub(super) enum Intent {
    Create(Input),
    Delete(ShellWorld),
    OpenCodex(CodexOpenTarget),
    Reconnect(WorldIdentity),
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) id: ActionId,
    pub(super) intent: Intent,
}

#[derive(Clone, Debug)]
pub(super) struct Active {
    pub(super) entry: Entry,
    pub(super) phase: String,
    tail_cleared: bool,
}

pub(super) struct ShellActionQueue {
    queued: VecDeque<Entry>,
    active: Option<Active>,
    next_id: ActionId,
    visible: bool,
    removed: Vec<Entry>,
}

impl Default for ShellActionQueue {
    fn default() -> Self {
        Self {
            queued: VecDeque::new(),
            active: None,
            next_id: 0,
            visible: true,
            removed: Vec::new(),
        }
    }
}

impl ShellActionQueue {
    pub(super) fn enqueue(&mut self, intent: Intent) -> ActionId {
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("shell action ID overflow");
        let id = self.next_id;
        self.queued.push_back(Entry { id, intent });
        self.visible = true;
        id
    }

    pub(super) fn activate_next(&mut self, phase: impl Into<String>) -> Option<&Active> {
        if self.active.is_none() {
            self.active = self.queued.pop_front().map(|entry| Active {
                entry,
                phase: phase.into(),
                tail_cleared: false,
            });
        }
        self.active.as_ref()
    }

    #[cfg(test)]
    pub(super) fn active(&self) -> Option<&Active> {
        self.active.as_ref()
    }

    pub(super) fn has_work(&self) -> bool {
        self.active.is_some() || !self.queued.is_empty()
    }

    pub(super) fn running_work(&self) -> Vec<String> {
        let mut work = Vec::new();
        if let Some(active) = &self.active {
            work.push(format!(
                "{} ({})",
                label(&active.entry.intent),
                active.phase
            ));
        }
        work.extend(self.queued.iter().map(|entry| label(&entry.intent)));
        work
    }

    pub(super) fn is_active(&self, id: ActionId) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.entry.id == id)
    }

    pub(super) fn create_names(&self) -> impl Iterator<Item = &str> {
        self.queued
            .iter()
            .chain(self.active.iter().map(|active| &active.entry))
            .filter_map(|entry| match &entry.intent {
                Intent::Create(input) => Some(input.name.as_str()),
                _ => None,
            })
    }

    pub(super) fn update_phase(&mut self, id: ActionId, phase: impl Into<String>) -> bool {
        let Some(active) = self.active.as_mut().filter(|active| active.entry.id == id) else {
            return false;
        };
        active.phase = phase.into();
        true
    }

    pub(super) fn acknowledge(&mut self, id: ActionId, succeeded: bool) -> Option<Entry> {
        if !self
            .active
            .as_ref()
            .is_some_and(|active| active.entry.id == id)
        {
            return None;
        }
        let active = self.active.take()?;
        if !succeeded && !active.tail_cleared {
            self.clear_tail();
        }
        Some(active.entry)
    }

    pub(super) fn begin_cancellation(&mut self, id: ActionId) -> bool {
        let Some(active) = self.active.as_mut().filter(|active| active.entry.id == id) else {
            return false;
        };
        active.tail_cleared = true;
        self.clear_tail();
        true
    }

    pub(super) fn remove(&mut self, id: ActionId) -> bool {
        let Some(index) = self.queued.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let removed = self.queued.split_off(index);
        self.removed.extend(removed);
        true
    }

    pub(super) fn take_removed(&mut self) -> Vec<Entry> {
        std::mem::take(&mut self.removed)
    }

    fn clear_tail(&mut self) {
        self.removed.extend(self.queued.drain(..));
    }

    #[cfg(test)]
    pub(super) fn queued(&self) -> impl Iterator<Item = &Entry> {
        self.queued.iter()
    }

    pub(super) fn render(&self, frame: &mut Frame<'_>, outer: Rect, compact: bool) {
        if !self.visible || !self.has_work() {
            return;
        }
        let Some(area) = panel_area(outer, compact) else {
            return;
        };
        frame.render_widget(Clear, area);
        let queued_rows = if compact { 1 } else { 3 };
        let hidden = self.queued.len().saturating_sub(queued_rows);
        let title = if hidden == 0 {
            " Actions ".to_owned()
        } else {
            format!(" Actions · +{hidden} ")
        };
        frame.render_widget(
            Block::new()
                .borders(Borders::ALL)
                .title(title)
                .title(Line::from("×").alignment(Alignment::Right)),
            area,
        );
        let mut rows = Vec::new();
        if let Some(active) = &self.active {
            rows.push(ListItem::new(format!(
                "◌ {} — {}",
                label(&active.entry.intent),
                active.phase
            )));
        }
        rows.extend(
            self.queued
                .iter()
                .take(queued_rows)
                .enumerate()
                .map(|(index, entry)| {
                    ListItem::new(format!("{}. {}", index + 1, label(&entry.intent)))
                }),
        );
        let inner = area.inner(ratatui::layout::Margin::new(1, 1));
        frame.render_widget(List::new(rows), inner);
        let first_queued_row = inner.y + u16::from(self.active.is_some());
        for (index, _) in self.queued.iter().take(queued_rows).enumerate() {
            frame.render_widget(
                Paragraph::new("×").alignment(Alignment::Right),
                Rect::new(
                    inner.x,
                    first_queued_row + u16::try_from(index).unwrap_or(0),
                    inner.width,
                    1,
                ),
            );
        }
    }

    pub(super) fn handle_mouse(&mut self, event: &Event, outer: Rect, compact: bool) -> bool {
        if !self.visible || !self.has_work() {
            return false;
        }
        let Event::Mouse(mouse) = event else {
            return false;
        };
        let Some(area) = panel_area(outer, compact) else {
            return false;
        };
        let position = (mouse.column, mouse.row).into();
        if !area.contains(position) {
            return false;
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }
        if mouse.row == area.y && mouse.column == area.right().saturating_sub(2) {
            self.visible = false;
            return true;
        }
        let first_queued_row = area.y + 1 + u16::from(self.active.is_some());
        let index = usize::from(mouse.row.saturating_sub(first_queued_row));
        if mouse.column == area.right().saturating_sub(2) {
            if let Some(id) = self.queued.get(index).map(|entry| entry.id) {
                self.remove(id);
            }
        }
        true
    }
}

fn panel_area(outer: Rect, compact: bool) -> Option<Rect> {
    let width = (if compact { 52 } else { 78 }).min(outer.width.saturating_sub(2));
    if width < 28 || outer.height < 4 {
        return None;
    }
    let height = if compact { 4 } else { 6 };
    Some(Rect::new(
        outer.right().saturating_sub(width + 1),
        outer.y.saturating_add(1),
        width,
        height.min(outer.height.saturating_sub(1)),
    ))
}

pub(super) fn label(intent: &Intent) -> String {
    match intent {
        Intent::Create(input) => format!("Create {}.{}", input.context, input.name),
        Intent::Delete(world) => format!("Delete {}", world.name),
        Intent::OpenCodex(target) => format!("Open Codex in {}", target.context),
        Intent::Reconnect(identity) => format!("Reconnect {}", identity.context),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEvent};
    use uuid::Uuid;

    fn reconnect(id: u128) -> Intent {
        Intent::Reconnect(WorldIdentity {
            context: "local".into(),
            id: Uuid::from_u128(id),
        })
    }

    #[test]
    fn actions_run_fifo_and_a_failure_clears_the_old_tail() {
        let mut queue = ShellActionQueue::default();
        let first = queue.enqueue(reconnect(1));
        let second = queue.enqueue(reconnect(2));

        assert_eq!(queue.activate_next("Connecting").unwrap().entry.id, first);
        assert!(queue.acknowledge(first, false).is_some());
        assert!(queue.activate_next("Connecting").is_none());
        assert!(queue.queued().next().is_none());
        assert_ne!(first, second);
    }

    #[test]
    fn removing_a_waiting_action_removes_its_tail() {
        let mut queue = ShellActionQueue::default();
        let first = queue.enqueue(reconnect(1));
        let second = queue.enqueue(reconnect(2));
        let third = queue.enqueue(reconnect(3));

        assert!(queue.remove(second));
        assert_eq!(
            queue.queued().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![first]
        );
        assert_ne!(second, third);
    }

    #[test]
    fn stale_events_do_not_change_the_active_action() {
        let mut queue = ShellActionQueue::default();
        let active = queue.enqueue(reconnect(1));
        let stale = queue.enqueue(reconnect(2));
        queue.activate_next("Connecting");

        assert!(!queue.update_phase(stale, "Ready"));
        assert!(queue.acknowledge(stale, true).is_none());
        assert_eq!(queue.active().unwrap().entry.id, active);
    }

    #[test]
    fn actions_added_after_cancellation_starts_form_a_fresh_tail() {
        let mut queue = ShellActionQueue::default();
        let active = queue.enqueue(reconnect(1));
        queue.enqueue(reconnect(2));
        queue.activate_next("Connecting");

        assert!(queue.begin_cancellation(active));
        let fresh = queue.enqueue(reconnect(3));
        assert!(queue.acknowledge(active, false).is_some());

        assert_eq!(queue.activate_next("Connecting").unwrap().entry.id, fresh);
    }

    #[test]
    fn clicking_a_waiting_rows_visible_close_removes_its_tail() {
        let outer = Rect::new(0, 0, 100, 30);
        let mut queue = ShellActionQueue::default();
        let active = queue.enqueue(reconnect(1));
        let remaining = queue.enqueue(reconnect(2));
        let removed = queue.enqueue(reconnect(3));
        queue.enqueue(reconnect(4));
        queue.activate_next("Connecting");
        let area = panel_area(outer, false).unwrap();
        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.right() - 2,
            row: area.y + 3,
            modifiers: KeyModifiers::NONE,
        });

        assert!(queue.handle_mouse(&event, outer, false));
        assert_eq!(queue.active().unwrap().entry.id, active);
        assert_eq!(
            queue.queued().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![remaining]
        );
        assert_ne!(remaining, removed);
    }

    #[test]
    fn running_work_describes_active_and_queued_actions() {
        let mut queue = ShellActionQueue::default();
        let active = queue.enqueue(reconnect(1));
        queue.enqueue(reconnect(2));
        queue.activate_next("Connecting");

        assert_eq!(
            queue.running_work(),
            [
                format!("Reconnect local ({})", queue.active().unwrap().phase),
                "Reconnect local".into(),
            ]
        );
        assert_eq!(queue.active().unwrap().entry.id, active);
    }
}
