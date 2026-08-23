use anyhow::{bail, Context, Result};
use nix::fcntl::{Flock, FlockArg};
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, CodexSessionEvent,
    CodexSessionEventKind, CodexSessionStartSource, CodexSessionStartSourceKind, TransportResponse,
    PROTOCOL_VERSION, RELAY_SOCKET,
};

const TIMEOUT: Duration = Duration::from_millis(250);
const HOOK_STATE_DIRECTORY: &str = ".local/state/wt/codex-hook-order";
const HOOK_ERROR_FILE: &str = ".local/state/wt/codex-session-report-error.json";

#[derive(Debug, Deserialize)]
struct HookPayload {
    session_id: Uuid,
    cwd: String,
    hook_event_name: HookEventName,
    source: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
enum HookEventName {
    SessionStart,
    PreCompact,
    PostCompact,
    UserPromptSubmit,
    Stop,
    SessionEnd,
}

#[derive(Default, Deserialize, Serialize)]
struct PaneEventOrder {
    current_generation: u64,
    next_sequence: u64,
    session_generations: BTreeMap<Uuid, u64>,
}

fn session_start_source(raw: String) -> CodexSessionStartSource {
    let kind = match raw.as_str() {
        "startup" => CodexSessionStartSourceKind::Startup,
        "resume" => CodexSessionStartSourceKind::Resume,
        "clear" => CodexSessionStartSourceKind::Clear,
        "compact" => CodexSessionStartSourceKind::Compact,
        _ => CodexSessionStartSourceKind::Other,
    };
    CodexSessionStartSource { kind, raw }
}

fn event_kind(event: HookEventName) -> CodexSessionEventKind {
    match event {
        HookEventName::SessionStart => CodexSessionEventKind::SessionStart,
        HookEventName::PreCompact => CodexSessionEventKind::PreCompact,
        HookEventName::PostCompact => CodexSessionEventKind::PostCompact,
        HookEventName::UserPromptSubmit => CodexSessionEventKind::UserPromptSubmit,
        HookEventName::Stop => CodexSessionEventKind::Stop,
        HookEventName::SessionEnd => CodexSessionEventKind::SessionEnd,
    }
}

pub(crate) fn report_hook() -> Result<()> {
    let payload: HookPayload = serde_json::from_reader(io::stdin()).context("decode Codex hook")?;
    let event_name = format!("{:?}", payload.hook_event_name);
    let session_id = payload.session_id;
    match report_hook_payload(payload) {
        Ok(()) => {
            clear_hook_error()?;
            Ok(())
        }
        Err(error) => {
            record_hook_error(&event_name, session_id, &error)?;
            Err(error)
        }
    }
}

fn report_hook_payload(payload: HookPayload) -> Result<()> {
    let session_start_source = match payload.hook_event_name {
        HookEventName::SessionStart => payload.source.map(session_start_source),
        _ => None,
    };
    let pane_id = env::var("WT_BYOBU_PANE")
        .or_else(|_| env::var("TMUX_PANE"))
        .context("Codex is not running in a WT Byobu pane")?;
    let tmux_session = match env::var("WT_BYOBU_SESSION") {
        Ok(value) => value,
        Err(_) => current_tmux_session(&pane_id)?,
    };
    let (pane_generation, pane_sequence) =
        pane_event_order(&pane_id, payload.session_id, payload.hook_event_name)?;
    let event = CodexSessionEvent {
        session_id: payload.session_id,
        cwd: payload.cwd,
        tmux_session,
        pane_id,
        kind: event_kind(payload.hook_event_name),
        pane_generation,
        pane_sequence,
        session_start_source,
    };
    let mut stream = UnixStream::connect(RELAY_SOCKET).context("connect to WT guest relay")?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    write_json_line(
        &mut stream,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::CodexSession { event },
        },
    )?;
    let response: TransportResponse = read_json_line(&mut stream)?;
    if !response.ok {
        bail!(
            "WT guest relay rejected Codex session report: {}",
            response.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct HookError<'a> {
    timestamp_unix_ms: u128,
    event_name: &'a str,
    session_id: Uuid,
    error: String,
}

fn hook_error_path() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("HOME").context("HOME is not set")?).join(HOOK_ERROR_FILE))
}

fn record_hook_error(event_name: &str, session_id: Uuid, error: &anyhow::Error) -> Result<()> {
    let path = hook_error_path()?;
    let parent = path
        .parent()
        .context("Codex hook error path has no parent")?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let record = HookError {
        timestamp_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        event_name,
        session_id,
        error: error
            .to_string()
            .chars()
            .filter(|character| !character.is_control())
            .take(512)
            .collect(),
    };
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, serde_json::to_vec(&record)?)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn clear_hook_error() -> Result<()> {
    let path = hook_error_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("clear Codex hook error"),
    }
}

fn pane_event_order(pane_id: &str, session_id: Uuid, event: HookEventName) -> Result<(u64, u64)> {
    let pane = pane_id
        .strip_prefix('%')
        .filter(|pane| !pane.is_empty() && pane.bytes().all(|byte| byte.is_ascii_digit()))
        .context("invalid Codex Byobu pane")?;
    let home = env::var_os("HOME").context("HOME is not set")?;
    let directory = PathBuf::from(home).join(HOOK_STATE_DIRECTORY);
    pane_event_order_at(&directory, pane, session_id, event)
}

fn pane_event_order_at(
    directory: &Path,
    pane: &str,
    session_id: Uuid,
    event: HookEventName,
) -> Result<(u64, u64)> {
    fs::create_dir_all(directory)
        .with_context(|| format!("create Codex hook state directory {}", directory.display()))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("secure Codex hook state directory {}", directory.display()))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(directory.join(format!("{pane}.json")))
        .context("open Codex hook order state")?;
    let mut file = Flock::lock(file, FlockArg::LockExclusive)
        .map_err(|(_, error)| error)
        .context("lock Codex hook order")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .context("read Codex hook order")?;
    let mut order = if contents.is_empty() {
        PaneEventOrder::default()
    } else {
        serde_json::from_str(&contents).context("decode Codex hook order")?
    };
    let generation = match order.session_generations.get(&session_id) {
        Some(generation) => *generation,
        None if order.current_generation == 0 => {
            order.current_generation = 1;
            order.session_generations.insert(session_id, 1);
            1
        }
        None if matches!(event, HookEventName::SessionStart) => {
            order.current_generation = order
                .current_generation
                .checked_add(1)
                .context("Codex pane generation overflow")?;
            order
                .session_generations
                .insert(session_id, order.current_generation);
            order.current_generation
        }
        None => 0,
    };
    order.next_sequence = order
        .next_sequence
        .checked_add(1)
        .context("Codex pane event sequence overflow")?;
    file.seek(SeekFrom::Start(0))
        .context("rewind Codex hook order")?;
    file.set_len(0).context("truncate Codex hook order")?;
    serde_json::to_writer(&mut *file, &order).context("encode Codex hook order")?;
    file.sync_data().context("sync Codex hook order")?;
    Ok((generation, order.next_sequence))
}

fn current_tmux_session(pane_id: &str) -> Result<String> {
    let output = Command::new("/usr/bin/tmux")
        .args(["display-message", "-p", "-t", pane_id, "#{session_name}"])
        .output()
        .context("resolve current Byobu session")?;
    if !output.status.success() {
        bail!("Codex Byobu pane is not active");
    }
    let session = String::from_utf8(output.stdout).context("decode Byobu session name")?;
    Ok(session.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_hook_payloads() {
        let payload: HookPayload = serde_json::from_str(
            r#"{"session_id":"123e4567-e89b-12d3-a456-426614174000","cwd":"/home/wt/project","hook_event_name":"Stop","extra":"ignored"}"#,
        )
        .unwrap();

        assert_eq!(
            payload.session_id,
            Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()
        );
        assert!(matches!(payload.hook_event_name, HookEventName::Stop));
    }

    #[test]
    fn preserves_and_parses_session_start_source() {
        let payload: HookPayload = serde_json::from_str(
            r#"{"session_id":"123e4567-e89b-12d3-a456-426614174000","cwd":"/home/wt/project","hook_event_name":"SessionStart","source":"compact"}"#,
        )
        .unwrap();

        assert_eq!(
            payload.source.map(session_start_source),
            Some(CodexSessionStartSource {
                kind: CodexSessionStartSourceKind::Compact,
                raw: "compact".into(),
            })
        );
    }

    #[test]
    fn recognizes_compaction_lifecycle_events() {
        let pre_payload: HookPayload = serde_json::from_str(
            r#"{"session_id":"123e4567-e89b-12d3-a456-426614174000","cwd":"/home/wt/project","hook_event_name":"PreCompact"}"#,
        )
        .unwrap();
        let payload: HookPayload = serde_json::from_str(
            r#"{"session_id":"123e4567-e89b-12d3-a456-426614174000","cwd":"/home/wt/project","hook_event_name":"PostCompact"}"#,
        )
        .unwrap();

        assert_eq!(
            event_kind(pre_payload.hook_event_name),
            CodexSessionEventKind::PreCompact
        );
        assert_eq!(
            event_kind(payload.hook_event_name),
            CodexSessionEventKind::PostCompact
        );
    }

    #[test]
    fn assigns_a_stable_generation_and_monotonic_sequence_per_pane() {
        let temp = tempfile::tempdir().unwrap();
        let first = Uuid::new_v4();
        let replacement = Uuid::new_v4();

        assert_eq!(
            pane_event_order_at(temp.path(), "1", first, HookEventName::SessionStart).unwrap(),
            (1, 1)
        );
        assert_eq!(
            pane_event_order_at(temp.path(), "1", first, HookEventName::Stop).unwrap(),
            (1, 2)
        );
        assert_eq!(
            pane_event_order_at(temp.path(), "1", replacement, HookEventName::SessionStart,)
                .unwrap(),
            (2, 3)
        );
        assert_eq!(
            pane_event_order_at(temp.path(), "1", first, HookEventName::Stop).unwrap(),
            (1, 4)
        );
    }
}
