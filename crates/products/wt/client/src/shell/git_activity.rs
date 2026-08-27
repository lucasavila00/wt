use super::CONTEXT_REQUEST_TIMEOUT;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextWorld;
use wt_control_protocol::{
    ApiRequest, GitActivity, GitActivityKind, GitActivityQuery, Operation, Response, WorldId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RepositoryInteraction {
    pub(super) target: String,
    pub(super) wrote: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RepositoryActivity {
    Loading,
    Loaded(Vec<RepositoryInteraction>),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldGitActivity {
    pub(super) context: String,
    pub(super) world_id: WorldId,
    pub(super) repositories: Option<Vec<RepositoryInteraction>>,
}

pub(super) struct Refresh {
    pub(super) updates: Receiver<WorldGitActivity>,
    cancelled: Arc<AtomicBool>,
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

enum Command {
    Reconcile(Vec<ContextWorld>),
    Stop,
}

const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

impl Refresh {
    pub(super) fn start(config: ClientConfig, worlds: Vec<ContextWorld>) -> Self {
        let (updates_tx, updates) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-git-activity-refresh".into())
            .spawn(move || {
                let mut worlds = worlds;
                loop {
                    for world in &worlds {
                        if worker_cancelled.load(Ordering::Relaxed) {
                            return;
                        }
                        if updates_tx
                            .send(load_world(&config, world, &worker_cancelled))
                            .is_err()
                        {
                            return;
                        }
                    }
                    if !wait_for_next_refresh(&command_rx, &mut worlds, REFRESH_INTERVAL) {
                        return;
                    }
                }
            })
            .expect("start wt shell Git activity refresh worker");
        Self {
            updates,
            cancelled,
            commands,
            worker: Some(worker),
        }
    }

    pub(super) fn reconcile(&self, worlds: Vec<ContextWorld>) {
        let _ = self.commands.send(Command::Reconcile(worlds));
    }
}

fn wait_for_next_refresh(
    commands: &Receiver<Command>,
    worlds: &mut Vec<ContextWorld>,
    interval: Duration,
) -> bool {
    let deadline = Instant::now() + interval;
    loop {
        match commands.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Command::Reconcile(updated)) => {
                *worlds = updated;
                for command in commands.try_iter() {
                    match command {
                        Command::Reconcile(updated) => *worlds = updated,
                        Command::Stop => return false,
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return true,
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }
    }
}

impl Drop for Refresh {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.commands.send(Command::Stop);
        self.worker.take();
    }
}

fn load_world(
    config: &ClientConfig,
    world: &ContextWorld,
    cancelled: &AtomicBool,
) -> WorldGitActivity {
    let repositories = config.context(&world.context).and_then(|context| {
        let request = ApiRequest::new(Operation::ListGitActivity {
            query: GitActivityQuery::World {
                world_id: world.world.world_id,
                before_id: None,
            },
        });
        let response = wt_client::transport::call_with_timeout_until(
            context,
            &request,
            CONTEXT_REQUEST_TIMEOUT,
            cancelled,
        )
        .ok()?;
        let Response::GitActivity { activity } = response else {
            return None;
        };
        Some(recent_repositories(activity))
    });
    WorldGitActivity {
        context: world.context.clone(),
        world_id: world.world.world_id,
        repositories,
    }
}

fn recent_repositories(activity: Vec<GitActivity>) -> Vec<RepositoryInteraction> {
    let mut repositories = BTreeMap::new();
    for (position, entry) in activity.into_iter().enumerate() {
        let target = format!("{}/{}", entry.provider_host, entry.repository);
        let wrote = entry.kind == GitActivityKind::BranchUpdate
            || entry.git_service.as_deref() == Some("git-receive-pack");
        let summary = repositories
            .entry(target.clone())
            .or_insert((position, false));
        summary.1 |= wrote;
    }
    let mut repositories = repositories
        .into_iter()
        .map(|(target, (position, wrote))| (position, RepositoryInteraction { target, wrote }))
        .collect::<Vec<_>>();
    repositories.sort_by_key(|(position, _)| *position);
    repositories.truncate(3);
    repositories.sort_by_key(|(_, repository)| !repository.wrote);
    repositories
        .into_iter()
        .map(|(_, repository)| repository)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt_control_protocol::WorldName;

    fn activity(
        id: u64,
        repository: &str,
        kind: GitActivityKind,
        git_service: Option<&str>,
    ) -> GitActivity {
        GitActivity {
            id,
            world_id: WorldId::new(),
            world_name: WorldName::parse("world").unwrap(),
            recorded_at_unix_ms: id,
            kind,
            provider_host: "github.com".into(),
            repository: repository.into(),
            git_service: git_service.map(str::to_owned),
            branch: None,
            previous_oid: None,
            new_oid: None,
        }
    }

    #[test]
    fn keeps_three_recent_repositories_with_writes_first() {
        let repositories = recent_repositories(vec![
            activity(
                6,
                "read-newest",
                GitActivityKind::Service,
                Some("git-upload-pack"),
            ),
            activity(
                5,
                "write-recent",
                GitActivityKind::Service,
                Some("git-receive-pack"),
            ),
            activity(
                4,
                "read-recent",
                GitActivityKind::Service,
                Some("git-upload-pack"),
            ),
            activity(3, "write-old", GitActivityKind::BranchUpdate, None),
        ]);

        insta::assert_debug_snapshot!(repositories, @r###"
        [
            RepositoryInteraction {
                target: "github.com/write-recent",
                wrote: true,
            },
            RepositoryInteraction {
                target: "github.com/read-newest",
                wrote: false,
            },
            RepositoryInteraction {
                target: "github.com/read-recent",
                wrote: false,
            },
        ]
        "###);
    }

    #[test]
    fn inventory_reconciliation_does_not_reset_the_refresh_interval() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Command::Reconcile(Vec::new())).unwrap();
        let started = Instant::now();

        assert!(wait_for_next_refresh(
            &receiver,
            &mut Vec::new(),
            Duration::from_millis(25)
        ));
        assert!(started.elapsed() >= Duration::from_millis(20));
    }
}
