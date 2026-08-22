use super::{codex, CONTEXT_REQUEST_TIMEOUT, REFRESH_INTERVAL};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use wt_client::config::ClientConfig;
use wt_client::inventory;

pub(super) struct WorldRefresh {
    pub(super) updates: Receiver<WorldSnapshot>,
    pub(super) generation: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

pub(super) struct WorldSnapshot {
    pub(super) generation: u64,
    pub(super) instances: Vec<inventory::ContextInstance>,
    pub(super) failed: bool,
}

pub(super) struct CodexRefresh {
    pub(super) updates: Receiver<Vec<codex::CodexContextSnapshot>>,
    cancelled: Arc<AtomicBool>,
    commands: Sender<CodexRefreshCommand>,
    worker: Option<JoinHandle<()>>,
}

enum CodexRefreshCommand {
    Stop,
}

impl WorldRefresh {
    pub(super) fn start(config: ClientConfig) -> Self {
        let (updates_tx, updates) = mpsc::sync_channel(1);
        let (stop, stop_rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_generation = Arc::clone(&generation);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-world-refresh".into())
            .spawn(move || loop {
                if stop_rx.recv_timeout(REFRESH_INTERVAL).is_ok() {
                    break;
                }
                let generation = worker_generation.load(Ordering::Relaxed);
                let report = inventory::list_all_with_timeout(
                    &config,
                    CONTEXT_REQUEST_TIMEOUT,
                    &worker_cancelled,
                );
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match updates_tx.try_send(WorldSnapshot {
                    generation,
                    instances: report.instances,
                    failed: !report.failures.is_empty(),
                }) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
            })
            .expect("start wt shell world refresh worker");
        Self {
            updates,
            generation,
            cancelled,
            stop,
            worker: Some(worker),
        }
    }

    pub(super) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for WorldRefresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl CodexRefresh {
    pub(super) fn start(config: ClientConfig) -> Self {
        let (updates_tx, updates) = mpsc::sync_channel(1);
        let (commands, command_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-codex-refresh".into())
            .spawn(move || loop {
                let snapshot = codex::load_snapshots(&config, &worker_cancelled);
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match updates_tx.try_send(snapshot) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
                match command_rx.recv_timeout(REFRESH_INTERVAL) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(CodexRefreshCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break
                    }
                }
            })
            .expect("start wt shell Codex refresh worker");
        Self {
            updates,
            cancelled,
            commands,
            worker: Some(worker),
        }
    }
}

impl Drop for CodexRefresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.commands.send(CodexRefreshCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn take_current_snapshot(
    updates: &Receiver<WorldSnapshot>,
    generation: u64,
) -> Option<WorldSnapshot> {
    updates
        .try_iter()
        .last()
        .filter(|snapshot| snapshot.generation == generation)
}
