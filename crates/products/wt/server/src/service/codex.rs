use super::{map_store_error, AgentToolGateway, Service};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use wt_control_protocol::{
    ApiError, ByobuTarget, CodexSession, CodexSessionObservation, CodexSessionState, ErrorCode,
    InstanceName, Response,
};
use wt_retained_worlds::WorldWorker;

const MAX_SESSION_META_BYTES: u64 = 64 * 1024;
const MAX_SESSION_TITLE_SCAN_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Debug)]
struct Rollout {
    session_id: Uuid,
    updated_at_unix_ms: i64,
    title: Option<String>,
}

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_codex_sessions(&self, owner: &str) -> Result<Response, ApiError> {
        let (rollouts, warnings) =
            discover_rollouts(&self.codex_sessions_path).map_err(|error| {
                ApiError::new(
                    ErrorCode::Internal,
                    format!("discover Codex sessions: {error}"),
                )
            })?;
        for warning in warnings {
            eprintln!("wt-server: Codex session discovery: {warning}");
        }
        let reports = self
            .store
            .list_codex_session_reports(owner)
            .map_err(map_store_error)?;
        Ok(Response::CodexSessions {
            sessions: merge_sessions(rollouts, reports)?,
        })
    }
}

fn merge_sessions(
    rollouts: Vec<Rollout>,
    reports: Vec<wt_workload_registry::CodexSessionReport>,
) -> Result<Vec<CodexSession>, ApiError> {
    let mut sessions = rollouts
        .into_iter()
        .map(|rollout| {
            (
                rollout.session_id,
                CodexSession {
                    session_id: rollout.session_id,
                    title: rollout.title,
                    rollout_updated_at_unix_ms: Some(rollout.updated_at_unix_ms),
                    observations: Vec::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for report in reports {
        let session = sessions.entry(report.session_id).or_insert(CodexSession {
            session_id: report.session_id,
            title: None,
            rollout_updated_at_unix_ms: None,
            observations: Vec::new(),
        });
        session.observations.push(CodexSessionObservation {
            world_id: report.world_id,
            world_name: InstanceName::parse(report.world_name).map_err(|error| {
                ApiError::new(
                    ErrorCode::Internal,
                    format!("invalid session world: {error}"),
                )
            })?,
            cwd: report.cwd,
            repository_root: report.repository_root,
            repository_url: report.repository_url,
            git_branch: report.git_branch,
            state: match report.state {
                wt_workload_registry::CodexSessionState::Unknown => CodexSessionState::Unknown,
                wt_workload_registry::CodexSessionState::Working => CodexSessionState::Working,
                wt_workload_registry::CodexSessionState::NeedsAttention => {
                    CodexSessionState::NeedsAttention
                }
                wt_workload_registry::CodexSessionState::Inactive => CodexSessionState::Inactive,
            },
            session_start_source: report.session_start_source,
            target: ByobuTarget {
                tmux_session: report.tmux_session,
                pane_id: report.pane_id,
            },
            received_at_unix_ms: report.received_at_unix_ms,
        });
    }
    for session in sessions.values_mut() {
        session.observations.sort_by(|left, right| {
            right
                .received_at_unix_ms
                .cmp(&left.received_at_unix_ms)
                .then_with(|| left.world_name.cmp(&right.world_name))
        });
    }
    let mut sessions = sessions.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        session_updated_at(right)
            .cmp(&session_updated_at(left))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    Ok(sessions)
}

fn session_updated_at(session: &CodexSession) -> i64 {
    session
        .observations
        .first()
        .map(|observation| observation.received_at_unix_ms)
        .into_iter()
        .chain(session.rollout_updated_at_unix_ms)
        .max()
        .unwrap_or_default()
}

fn discover_rollouts(root: &Path) -> Result<(Vec<Rollout>, Vec<String>), String> {
    if !root.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut paths = Vec::new();
    collect_rollouts(root, &mut paths)?;
    paths.sort();
    let mut rollouts = BTreeMap::new();
    let mut warnings = Vec::new();
    for path in paths {
        match read_rollout(&path) {
            Ok(Some(rollout)) => {
                rollouts
                    .entry(rollout.session_id)
                    .and_modify(|current: &mut Rollout| {
                        current.updated_at_unix_ms =
                            current.updated_at_unix_ms.max(rollout.updated_at_unix_ms)
                    })
                    .or_insert(rollout);
            }
            Ok(None) => {}
            Err(error) => warnings.push(format!("skip {}: {error}", path.display())),
        }
    }
    Ok((rollouts.into_values().collect(), warnings))
}

fn collect_rollouts(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read session directory {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            collect_rollouts(&entry.path(), paths)?;
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy().starts_with("rollout-")
            && entry
                .path()
                .extension()
                .is_some_and(|value| value == "jsonl")
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn read_rollout(path: &Path) -> Result<Option<Rollout>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    (&mut reader)
        .take(MAX_SESSION_META_BYTES + 1)
        .read_line(&mut first)
        .map_err(|error| error.to_string())?;
    if first.is_empty() {
        return Err("rollout is empty".into());
    }
    if first.len() as u64 > MAX_SESSION_META_BYTES {
        return Err("first rollout record is too large".into());
    }
    let record: SessionRecord =
        serde_json::from_str(&first).map_err(|error| format!("invalid first record: {error}"))?;
    if record.kind != "session_meta" {
        return Err("first rollout record is not session_meta".into());
    }
    if record.payload.is_subagent() {
        return Ok(None);
    }
    let id = record
        .payload
        .id
        .ok_or_else(|| "session_meta has no thread ID".to_owned())?;
    let session_id = Uuid::parse_str(&id).map_err(|_| "thread ID is not a UUID".to_owned())?;
    if id != session_id.hyphenated().to_string() {
        return Err("thread ID is not canonical".into());
    }
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| error.to_string())?;
    let title = reader
        .take(MAX_SESSION_TITLE_SCAN_BYTES)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find_map(session_title);
    Ok(Some(Rollout {
        session_id,
        updated_at_unix_ms: unix_time_from(modified)?,
        title,
    }))
}

fn session_title(record: Value) -> Option<String> {
    let payload = record.get("payload")?;
    if record.get("type")?.as_str()? != "response_item"
        || payload.get("type")?.as_str()? != "message"
        || payload.get("role")?.as_str()? != "user"
    {
        return None;
    }
    let text = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then(|| normalized.chars().take(160).collect())
}

fn unix_time_from(time: SystemTime) -> Result<i64, String> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
    )
    .map_err(|_| "timestamp is too large".to_owned())
}

#[derive(Deserialize)]
struct SessionRecord {
    #[serde(rename = "type")]
    kind: String,
    payload: SessionPayload,
}

#[derive(Deserialize)]
struct SessionPayload {
    id: Option<String>,
    #[serde(default)]
    source: Value,
}

impl SessionPayload {
    fn is_subagent(&self) -> bool {
        self.source.as_object().is_some_and(|source| {
            source.contains_key("subagent") || source.contains_key("subAgent")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_top_level_sessions_and_skips_subagents() {
        let temp = tempfile::tempdir().unwrap();
        let session_id = Uuid::new_v4();
        fs::write(
            temp.path().join("rollout-main.jsonl"),
            format!(r#"{{"type":"session_meta","payload":{{"id":"{session_id}","source":{{}}}}}}"#),
        )
        .unwrap();
        fs::write(
            temp.path().join("rollout-sub.jsonl"),
            format!(
                r#"{{"type":"session_meta","payload":{{"id":"{}","source":{{"subagent":{{}}}}}}}}"#,
                Uuid::new_v4()
            ),
        )
        .unwrap();

        let (rollouts, warnings) = discover_rollouts(temp.path()).unwrap();

        assert!(warnings.is_empty());
        assert_eq!(rollouts.len(), 1);
        assert_eq!(rollouts[0].session_id, session_id);
    }

    #[test]
    fn reads_the_first_user_message_as_the_session_title() {
        let temp = tempfile::tempdir().unwrap();
        let session_id = Uuid::new_v4();
        fs::write(
            temp.path().join("rollout-main.jsonl"),
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"source\":{{}}}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"Improve  the session\\n cards\"}}]}}}}\n"
            ),
        )
        .unwrap();

        let (rollouts, warnings) = discover_rollouts(temp.path()).unwrap();

        assert!(warnings.is_empty());
        assert_eq!(
            rollouts[0].title.as_deref(),
            Some("Improve the session cards")
        );
    }

    #[test]
    fn reports_keep_the_observed_state_and_complete_target() {
        let session_id = Uuid::new_v4();
        let sessions = merge_sessions(
            vec![Rollout {
                session_id,
                updated_at_unix_ms: 1,
                title: Some("Improve session cards".into()),
            }],
            vec![wt_workload_registry::CodexSessionReport {
                world_id: Uuid::new_v4(),
                world_name: "example".into(),
                session_id,
                cwd: "/home/wt/project".into(),
                repository_root: Some("/home/wt/project".into()),
                repository_url: Some("git@github.com:acme/project.git".into()),
                git_branch: Some("wt/cards".into()),
                tmux_session: "wt-host".into(),
                pane_id: "%1".into(),
                state: wt_workload_registry::CodexSessionState::Working,
                session_start_source: None,
                received_at_unix_ms: 2,
            }],
        )
        .unwrap();

        assert_eq!(sessions[0].observations.len(), 1);
        assert_eq!(
            sessions[0].observations[0].state,
            CodexSessionState::Working
        );
        assert_eq!(sessions[0].observations[0].target.pane_id, "%1");
    }

    #[test]
    fn preserves_every_world_observation_for_one_session() {
        let session_id = Uuid::new_v4();
        let reports = [("first", "%1", 10), ("second", "%2", 20)]
            .into_iter()
            .map(|(world_name, pane_id, received_at_unix_ms)| {
                wt_workload_registry::CodexSessionReport {
                    world_id: Uuid::new_v4(),
                    world_name: world_name.into(),
                    session_id,
                    cwd: "/home/wt/project".into(),
                    repository_root: None,
                    repository_url: None,
                    git_branch: None,
                    tmux_session: "wt-host".into(),
                    pane_id: pane_id.into(),
                    state: wt_workload_registry::CodexSessionState::Working,
                    session_start_source: None,
                    received_at_unix_ms,
                }
            })
            .collect();

        let sessions = merge_sessions(Vec::new(), reports).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].observations.len(), 2);
        assert_eq!(sessions[0].observations[0].world_name.as_str(), "second");
        assert_eq!(sessions[0].observations[1].world_name.as_str(), "first");
    }
}
