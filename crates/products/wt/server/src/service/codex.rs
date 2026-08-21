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
    ApiError, CodexSession, CodexSessionState, CodexSessionTarget, ErrorCode, InstanceName,
    Response,
};
use wt_retained_worlds::WorldWorker;

const MAX_SESSION_META_BYTES: u64 = 64 * 1024;
const LIVE_REPORT_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
struct Rollout {
    session_id: Uuid,
    updated_at: i64,
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
        let now = unix_time().map_err(|error| ApiError::new(ErrorCode::Internal, error))?;
        Ok(Response::CodexSessions {
            sessions: merge_sessions(rollouts, reports, now)?,
        })
    }
}

fn merge_sessions(
    rollouts: Vec<Rollout>,
    reports: Vec<wt_workload_registry::CodexSessionReport>,
    now: i64,
) -> Result<Vec<CodexSession>, ApiError> {
    let mut sessions = rollouts
        .into_iter()
        .map(|rollout| {
            (
                rollout.session_id,
                CodexSession {
                    session_id: rollout.session_id,
                    updated_at: rollout.updated_at,
                    state: CodexSessionState::Unknown,
                    cwd: None,
                    target: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut newest_reports = BTreeMap::new();
    for report in reports {
        newest_reports.entry(report.session_id).or_insert(report);
    }
    for (session_id, report) in newest_reports {
        let session = sessions.entry(session_id).or_insert(CodexSession {
            session_id,
            updated_at: report.received_at,
            state: CodexSessionState::Unknown,
            cwd: None,
            target: None,
        });
        session.updated_at = session.updated_at.max(report.received_at);
        session.cwd = Some(report.cwd);
        if now.saturating_sub(report.received_at) > LIVE_REPORT_TTL_SECONDS {
            continue;
        }
        session.state = match report.state {
            wt_workload_registry::CodexSessionState::Unknown => CodexSessionState::Unknown,
            wt_workload_registry::CodexSessionState::Working => CodexSessionState::Working,
            wt_workload_registry::CodexSessionState::NeedsAttention => {
                CodexSessionState::NeedsAttention
            }
            wt_workload_registry::CodexSessionState::Inactive => CodexSessionState::Inactive,
        };
        if session.state != CodexSessionState::Inactive {
            session.target = Some(CodexSessionTarget {
                world_id: report.world_id,
                world_name: InstanceName::parse(report.world_name).map_err(|error| {
                    ApiError::new(
                        ErrorCode::Internal,
                        format!("invalid session world: {error}"),
                    )
                })?,
                tmux_session: report.tmux_session,
                pane_id: report.pane_id,
            });
        }
    }
    let mut sessions = sessions.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(sessions)
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
                        current.updated_at = current.updated_at.max(rollout.updated_at)
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
    let mut first = String::new();
    BufReader::new(file)
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
    Ok(Some(Rollout {
        session_id,
        updated_at: unix_time_from(modified)?,
    }))
}

fn unix_time() -> Result<i64, String> {
    unix_time_from(SystemTime::now())
}

fn unix_time_from(time: SystemTime) -> Result<i64, String> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs(),
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
    fn expired_reports_lose_their_target() {
        let session_id = Uuid::new_v4();
        let sessions = merge_sessions(
            vec![Rollout {
                session_id,
                updated_at: 1,
            }],
            vec![wt_workload_registry::CodexSessionReport {
                world_id: Uuid::new_v4(),
                world_name: "example".into(),
                session_id,
                cwd: "/workspace".into(),
                tmux_session: "wt-app".into(),
                pane_id: "%1".into(),
                state: wt_workload_registry::CodexSessionState::Working,
                received_at: 2,
            }],
            2 + LIVE_REPORT_TTL_SECONDS + 1,
        )
        .unwrap();

        assert_eq!(sessions[0].state, CodexSessionState::Unknown);
        assert_eq!(sessions[0].target, None);
    }
}
