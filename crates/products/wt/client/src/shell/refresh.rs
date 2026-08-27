use super::{git_activity, pane, CONTEXT_REQUEST_TIMEOUT, REFRESH_INTERVAL};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
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
    pub(super) worlds: Vec<inventory::ContextWorld>,
    pub(super) git_activity: Vec<git_activity::WorldGitActivity>,
    pub(super) capacity: wt_control_protocol::ResourceCapacity,
    pub(super) failures: Vec<String>,
    pub(super) ssh_sync_error: Option<String>,
}

pub(super) struct PaneRefresh {
    pub(super) updates: Receiver<Vec<pane::PaneContextSnapshot>>,
    cancelled: Arc<AtomicBool>,
    commands: Sender<PaneRefreshCommand>,
    worker: Option<JoinHandle<()>>,
}

enum PaneRefreshCommand {
    Stop,
}

pub(super) fn updated_at() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .expect("zero is a valid nanosecond")
        .format(&Rfc3339)
        .expect("UTC timestamps support RFC 3339")
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
                let git_activity = git_activity::load(&config, &report.worlds, &worker_cancelled);
                let failures: Vec<String> = report
                    .failures
                    .into_iter()
                    .map(|failure| {
                        failure
                            .to_string()
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_owned()
                    })
                    .collect();
                let ssh_sync_error = if failures.is_empty() {
                    wt_client::ssh::sync(&config, &report.worlds)
                        .err()
                        .map(|error| format!("SSH inventory synchronization failed: {error:#}"))
                } else {
                    None
                };
                match updates_tx.try_send(WorldSnapshot {
                    generation,
                    worlds: report.worlds,
                    git_activity,
                    capacity: report.capacity,
                    failures,
                    ssh_sync_error,
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
        self.worker.take();
    }
}

impl PaneRefresh {
    pub(super) fn start(config: ClientConfig) -> Self {
        let (updates_tx, updates) = mpsc::sync_channel(1);
        let (commands, command_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-pane-refresh".into())
            .spawn(move || loop {
                let snapshot = pane::load_snapshots(&config, &worker_cancelled);
                if worker_cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match updates_tx.try_send(snapshot) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
                match command_rx.recv_timeout(REFRESH_INTERVAL) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(PaneRefreshCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break
                    }
                }
            })
            .expect("start wt shell pane refresh worker");
        Self {
            updates,
            cancelled,
            commands,
            worker: Some(worker),
        }
    }
}

impl Drop for PaneRefresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.commands.send(PaneRefreshCommand::Stop);
        self.worker.take();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn dropping_a_refresh_worker_does_not_wait_for_it() {
        let (_updates_sender, updates) = mpsc::sync_channel(1);
        let (stop, _stop_receiver) = mpsc::channel();
        let worker = thread::spawn(|| thread::sleep(Duration::from_millis(250)));
        let refresh = WorldRefresh {
            updates,
            generation: Arc::new(AtomicU64::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
            stop,
            worker: Some(worker),
        };

        let started = Instant::now();
        drop(refresh);

        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
