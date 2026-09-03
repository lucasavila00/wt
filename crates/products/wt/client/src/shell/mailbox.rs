use super::model::ShellWorld;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use wt_client::config::Context;
use wt_control_protocol::WorldMail;

#[derive(Debug)]
pub(super) struct LoadResult {
    request_id: u64,
    result: Result<Vec<WorldMail>, String>,
}

pub(super) struct MailboxWorker {
    next_request_id: AtomicU64,
    sender: Sender<LoadResult>,
    receiver: Receiver<LoadResult>,
}

impl Default for MailboxWorker {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            next_request_id: AtomicU64::new(1),
            sender,
            receiver,
        }
    }
}

impl MailboxWorker {
    pub(super) fn start(&self, context: Context, world: &ShellWorld) -> u64 {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let world_id = world.identity.world_id;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result =
                crate::messages::list_world(&context, world_id).map_err(|error| error.to_string());
            let _ = sender.send(LoadResult { request_id, result });
        });
        request_id
    }

    pub(super) fn try_recv(&self) -> Option<LoadResult> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Debug)]
pub(super) struct MailboxView {
    request_id: u64,
    world_name: String,
    state: MailboxState,
    scroll: u16,
}

#[derive(Debug)]
enum MailboxState {
    Loading,
    Loaded(Vec<WorldMail>),
    Failed(String),
}

impl MailboxView {
    pub(super) fn loading(request_id: u64, world: ShellWorld) -> Self {
        Self {
            request_id,
            world_name: world.name,
            state: MailboxState::Loading,
            scroll: 0,
        }
    }

    pub(super) fn apply(&mut self, result: LoadResult) -> bool {
        if result.request_id != self.request_id {
            return false;
        }
        self.state = match result.result {
            Ok(messages) => MailboxState::Loaded(messages),
            Err(error) => MailboxState::Failed(error),
        };
        true
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers != KeyModifiers::NONE {
            return false;
        }
        match key.code {
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            _ => return false,
        }
        true
    }

    pub(super) fn render(&self, frame: &mut Frame<'_>) {
        let outer = frame.area();
        let width = 110.min(outer.width);
        let height = 24.min(outer.height);
        let area = Rect::new(
            outer.x + outer.width.saturating_sub(width) / 2,
            outer.y + outer.height.saturating_sub(height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, area);
        let block = Block::new()
            .borders(Borders::ALL)
            .title(format!(" Messages · {} ", self.world_name))
            .title_bottom(" ↑/↓ scroll · Esc close ");
        let inner = block.inner(area).inner(Margin::new(1, 0));
        frame.render_widget(block, area);
        let text = match &self.state {
            MailboxState::Loading => "Loading messages…".to_owned(),
            MailboxState::Failed(error) => format!("Could not load messages: {error}"),
            MailboxState::Loaded(messages) if messages.is_empty() => {
                "No retained messages.".to_owned()
            }
            MailboxState::Loaded(messages) => {
                let mut lines = vec![
                    "TIME (UNIX MS)   WINDOW                                MESSAGE".to_owned(),
                ];
                lines.extend(messages.iter().map(|mail| {
                    format!(
                        "{:<16} {}  {}",
                        mail.created_at_unix_ms,
                        mail.window_id,
                        mail.message.replace('\n', "\\n").replace('\r', "\\r")
                    )
                }));
                lines.join("\n")
            }
        };
        frame.render_widget(
            Paragraph::new(text)
                .scroll((self.scroll, 0))
                .style(Style::new().add_modifier(Modifier::DIM)),
            inner,
        );
    }
}
