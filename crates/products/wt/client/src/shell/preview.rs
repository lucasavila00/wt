use super::control::{CodexCardIdentity, CodexOpenTarget};
use super::model::ShellModel;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const WORKERS: usize = 4;
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

struct Job {
    target: CodexOpenTarget,
    alias: String,
    rows: u16,
    columns: u16,
}

struct Result {
    identity: CodexCardIdentity,
    rows: u16,
    columns: u16,
    output: Option<Vec<u8>>,
}

pub(super) struct PreviewSet {
    jobs: mpsc::Sender<Job>,
    results: mpsc::Receiver<Result>,
    screens: BTreeMap<CodexCardIdentity, vt100::Parser>,
    inflight: BTreeSet<CodexCardIdentity>,
    last_schedule: Option<Instant>,
}

impl PreviewSet {
    pub(super) fn new() -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let (sender, results) = mpsc::channel();
        for _ in 0..WORKERS {
            let receiver = Arc::clone(&receiver);
            let sender = sender.clone();
            thread::spawn(move || loop {
                let Ok(job) = receiver.lock().expect("preview queue poisoned").recv() else {
                    return;
                };
                let result = capture(&job);
                let _ = sender.send(Result {
                    identity: job.target.identity,
                    rows: job.rows,
                    columns: job.columns,
                    output: result,
                });
            });
        }
        Self {
            jobs,
            results,
            screens: BTreeMap::new(),
            inflight: BTreeSet::new(),
            last_schedule: None,
        }
    }

    pub(super) fn schedule(&mut self, model: &ShellModel, area: ratatui::layout::Rect) {
        if self
            .last_schedule
            .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.last_schedule = Some(Instant::now());
        let (rows, columns) = super::live::preview_size(area);
        let current = model
            .control()
            .codex()
            .iter()
            .map(|card| card.identity.clone())
            .collect::<BTreeSet<_>>();
        self.screens
            .retain(|identity, _| current.contains(identity));
        for card in model
            .control()
            .codex()
            .iter()
            .skip(model.control().codex_offset())
            .take(super::live::visible(area))
        {
            let Some(target) = card.open_target() else {
                continue;
            };
            let Some((_, alias)) = model.focus_route(&target) else {
                continue;
            };
            if !self.inflight.insert(target.identity.clone()) {
                continue;
            }
            let _ = self.jobs.send(Job {
                target,
                alias: alias.into(),
                rows,
                columns,
            });
        }
    }

    pub(super) fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.results.try_recv() {
            self.inflight.remove(&result.identity);
            let Some(output) = result.output else {
                continue;
            };
            let mut parser = vt100::Parser::new(result.rows, result.columns, 0);
            parser.process(b"\x1b[2J\x1b[H");
            parser.process(&output);
            self.screens.insert(result.identity, parser);
            changed = true;
        }
        changed
    }

    pub(super) fn screen(&self, identity: &CodexCardIdentity) -> Option<&vt100::Screen> {
        self.screens.get(identity).map(|parser| parser.screen())
    }

    #[cfg(test)]
    pub(super) fn insert(
        &mut self,
        identity: CodexCardIdentity,
        rows: u16,
        columns: u16,
        bytes: &[u8],
    ) {
        let mut parser = vt100::Parser::new(rows, columns, 0);
        parser.process(bytes);
        self.screens.insert(identity, parser);
    }
}

fn capture(job: &Job) -> Option<Vec<u8>> {
    let mut command = Command::new("ssh");
    command
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "BatchMode=yes",
            "--",
        ])
        .arg(&job.alias)
        .args([
            "/usr/local/bin/wt-codex-integration",
            "capture-pane",
            &job.target.session_id.to_string(),
            &job.target.tmux_session,
            &job.target.pane_id,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let output = wt_client::transport::wait_with_output_timeout(
        command.spawn().ok()?,
        Duration::from_secs(10),
    )
    .ok()?;
    (output.status.success() && output.stdout.len() <= MAX_CAPTURE_BYTES).then_some(output.stdout)
}
