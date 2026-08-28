use super::CONTEXT_REQUEST_TIMEOUT;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextWorld;
use wt_control_protocol::{
    ApiRequest, GitActivity, GitActivityKind, GitActivityQuery, Operation, Response, WorldId,
    WtToolsActivity, WtToolsActivityQuery,
};

const MAX_ACTIONS: usize = 5;
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ActionLog {
    Loading,
    Loaded(Vec<String>),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldActionLog {
    pub(super) context: String,
    pub(super) world_id: WorldId,
    pub(super) actions: Option<Vec<String>>,
}

pub(super) struct Refresh {
    pub(super) updates: Receiver<WorldActionLog>,
    cancelled: Arc<AtomicBool>,
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

enum Command {
    Reconcile(Vec<ContextWorld>),
    Stop,
}

impl Refresh {
    pub(super) fn start(config: ClientConfig, worlds: Vec<ContextWorld>) -> Self {
        let (updates_tx, updates) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("wt-shell-action-log-refresh".into())
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
            .expect("start wt shell action log refresh worker");
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
) -> WorldActionLog {
    let actions = config.context(&world.context).and_then(|context| {
        let git = call(
            context,
            Operation::ListGitActivity {
                query: GitActivityQuery::World {
                    world_id: world.world.world_id,
                    before_id: None,
                },
            },
            cancelled,
        )?;
        let tools = call(
            context,
            Operation::ListWtToolsActivity {
                query: WtToolsActivityQuery::World {
                    world_id: world.world.world_id,
                    before_id: None,
                },
            },
            cancelled,
        )?;
        let Response::GitActivity { activity: git } = git else {
            return None;
        };
        let Response::WtToolsActivity { activity: tools } = tools else {
            return None;
        };
        Some(recent_actions(git, tools))
    });
    WorldActionLog {
        context: world.context.clone(),
        world_id: world.world.world_id,
        actions,
    }
}

fn call(
    context: &wt_client::config::Context,
    operation: Operation,
    cancelled: &AtomicBool,
) -> Option<Response> {
    wt_client::transport::call_with_timeout_until(
        context,
        &ApiRequest::new(operation),
        CONTEXT_REQUEST_TIMEOUT,
        cancelled,
    )
    .ok()
}

fn recent_actions(git: Vec<GitActivity>, tools: Vec<WtToolsActivity>) -> Vec<String> {
    let mut actions = git
        .into_iter()
        .filter_map(git_action)
        .chain(tools.into_iter().map(tool_action))
        .collect::<Vec<_>>();
    actions.sort_by_key(|(recorded_at, _)| std::cmp::Reverse(*recorded_at));
    actions.truncate(MAX_ACTIONS);
    actions.into_iter().map(|(_, action)| action).collect()
}

fn git_action(activity: GitActivity) -> Option<(u64, String)> {
    let target = format!("{}/{}", activity.provider_host, activity.repository);
    let summary = match (activity.kind, activity.git_service.as_deref()) {
        (GitActivityKind::BranchUpdate, _) => format!(
            "Git: pushed {} to {target}",
            activity.branch.as_deref().unwrap_or("a branch")
        ),
        (GitActivityKind::Service, Some("git-upload-pack")) => {
            format!("Git: fetched from {target}")
        }
        (GitActivityKind::Service, Some("git-receive-pack")) => return None,
        (GitActivityKind::Service, _) => format!("Git: accessed {target}"),
    };
    Some((activity.recorded_at_unix_ms, summary))
}

fn tool_action(activity: WtToolsActivity) -> (u64, String) {
    let target = format!("{}/{}", activity.provider_host, activity.repository);
    let request = if activity.provider_host.contains("gitlab") {
        "MR"
    } else if activity.provider_host.contains("github") {
        "PR"
    } else {
        "change request"
    };
    let handle = activity
        .change_request
        .as_deref()
        .map(|value| format!(" #{value}"))
        .unwrap_or_default();
    let branch = activity
        .branch
        .as_deref()
        .map(|value| format!(" for {value}"))
        .unwrap_or_default();
    let action = match activity.action.as_str() {
        "show_mr" => format!("viewed {request}{handle}"),
        "show_mr_for_branch" => format!("looked up {request}{branch}"),
        "show_run" => "viewed CI run".into(),
        "show_job" => "viewed CI job".into(),
        "list_threads" => format!("listed threads on {request}{handle}"),
        "list_comments" => format!("listed comments on {request}{handle}"),
        "show_comment" => format!("viewed comment on {request}{handle}"),
        "edit_comment" => format!("edited comment on {request}{handle}"),
        "delete_comment" => format!("deleted comment on {request}{handle}"),
        "list_ci" => "checked CI".into(),
        "list_jobs" => "listed CI jobs".into(),
        "log_job" => "read CI job log".into(),
        "wait_mr" => format!("waited for {request}{handle}"),
        "wait_run" => "waited for CI run".into(),
        "wait_job" => "waited for CI job".into(),
        "open_mr" => format!("opened {request}{handle}{branch}"),
        "set_mr" => format!("changed {request}{handle}"),
        "edit_mr" => format!("edited {request}{handle}"),
        "comment_mr" => format!("commented on {request}{handle}"),
        "reply_thread" => format!("replied on {request}{handle}"),
        "set_thread" => format!("updated thread on {request}{handle}"),
        "retry_job" => "retried CI job".into(),
        "cancel_job" => "cancelled CI job".into(),
        "cancel_run" => "cancelled CI run".into(),
        action => action.replace('_', " "),
    };
    (
        activity.recorded_at_unix_ms,
        format!("wtg: {action} · {target}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt_control_protocol::WorldName;

    fn git(
        id: u64,
        kind: GitActivityKind,
        service: Option<&str>,
        branch: Option<&str>,
    ) -> GitActivity {
        GitActivity {
            id,
            world_id: WorldId::new(),
            world_name: WorldName::parse("world").unwrap(),
            recorded_at_unix_ms: id,
            kind,
            provider_host: "github.com".into(),
            repository: "owner/repository".into(),
            git_service: service.map(str::to_owned),
            branch: branch.map(str::to_owned),
            previous_oid: None,
            new_oid: None,
        }
    }

    fn tools(
        id: u64,
        action: &str,
        branch: Option<&str>,
        change_request: Option<&str>,
    ) -> WtToolsActivity {
        WtToolsActivity {
            id,
            world_id: WorldId::new(),
            world_name: WorldName::parse("world").unwrap(),
            recorded_at_unix_ms: id,
            provider_host: "github.com".into(),
            repository: "owner/repository".into(),
            action: action.into(),
            branch: branch.map(str::to_owned),
            change_request: change_request.map(str::to_owned),
            request_json: "{}".into(),
            response_json: "{}".into(),
        }
    }

    #[test]
    fn combines_recent_git_and_wtg_actions_newest_first() {
        let actions = recent_actions(
            vec![
                git(5, GitActivityKind::BranchUpdate, None, Some("wt/topic")),
                git(4, GitActivityKind::Service, Some("git-receive-pack"), None),
                git(2, GitActivityKind::Service, Some("git-upload-pack"), None),
            ],
            vec![tools(6, "open_mr", Some("wt/topic"), Some("42"))],
        );

        insta::assert_debug_snapshot!(actions, @r###"
        [
            "wtg: opened PR #42 for wt/topic · github.com/owner/repository",
            "Git: pushed wt/topic to github.com/owner/repository",
            "Git: fetched from github.com/owner/repository",
        ]
        "###);
    }

    #[test]
    fn bounds_the_action_log() {
        let actions = recent_actions(
            (1..=8)
                .map(|id| git(id, GitActivityKind::Service, Some("git-upload-pack"), None))
                .collect(),
            Vec::new(),
        );

        assert_eq!(actions.len(), MAX_ACTIONS);
        assert_eq!(actions[0], "Git: fetched from github.com/owner/repository");
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
