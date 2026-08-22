use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;
use wt_workload_registry::{CodexSessionCatalogEntry, Store};

const MAX_SESSION_META_BYTES: u64 = 64 * 1024;
const MAX_MESSAGE_PREVIEW_BYTES: usize = 640;
const MAX_ROLLOUT_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_REFRESH_WARNINGS: usize = 32;

pub(super) fn refresh(store: &Store, root: &Path) -> Result<Vec<String>, String> {
    let _refresh = refresh_lock()
        .lock()
        .map_err(|_| "Codex session catalog refresh lock is poisoned".to_owned())?;
    if !root.exists() {
        store
            .retain_codex_session_catalog_paths(&BTreeSet::new())
            .map_err(|error| error.to_string())?;
        return Ok(Vec::new());
    }
    let cached = store
        .list_codex_session_catalog()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| (entry.rollout_path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut paths = Vec::new();
    collect_rollouts(root, &mut paths)?;
    paths.sort();
    let mut retained = BTreeSet::new();
    let mut warnings = Vec::new();
    for path in paths {
        let path_text = path.to_string_lossy().into_owned();
        match update_rollout(&path, cached.get(&path_text)) {
            Ok(Some(update)) => {
                retained.insert(path_text);
                for warning in update.warnings {
                    add_warning(&mut warnings, warning);
                }
                if update.changed {
                    if let Err(error) = store.upsert_codex_session_catalog(&update.entry) {
                        add_warning(&mut warnings, format!("cache {}: {error}", path.display()));
                    }
                }
            }
            Ok(None) => {}
            Err(error) if error == "subagent" => {}
            Err(error) => {
                retained.insert(path_text);
                add_warning(&mut warnings, format!("skip {}: {error}", path.display()));
            }
        }
    }
    store
        .retain_codex_session_catalog_paths(&retained)
        .map_err(|error| error.to_string())?;
    Ok(warnings)
}

fn add_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_REFRESH_WARNINGS {
        warnings.push(warning);
    } else if warnings.len() == MAX_REFRESH_WARNINGS {
        warnings.push("additional Codex session catalog warnings suppressed".into());
    }
}

fn refresh_lock() -> &'static Mutex<()> {
    static REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    REFRESH_LOCK.get_or_init(|| Mutex::new(()))
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

fn update_rollout(
    path: &Path,
    cached: Option<&CodexSessionCatalogEntry>,
) -> Result<Option<RolloutUpdate>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let length = metadata.len();
    let modification_time = metadata.modified().map_err(|error| error.to_string())?;
    let modified = unix_time_from(modification_time)?;
    let modified_nanos = unix_nanos_from(modification_time)?;
    let file_identity = format!("{}:{}", metadata.dev(), metadata.ino());
    if let Some(cached) = cached {
        if cached.rollout_file_identity == file_identity
            && cached.rollout_length == length
            && cached.rollout_modified_at_unix_ns == modified_nanos
        {
            return Ok(Some(RolloutUpdate::unchanged(cached.clone())));
        }
    }
    let can_append = cached.is_some_and(|entry| {
        entry.rollout_file_identity == file_identity
            && entry.rollout_length < length
            && entry.scan_offset <= length
            && entry.rollout_modified_at_unix_ns <= modified_nanos
    });
    let mut entry = if can_append {
        cached.cloned().expect("checked cached entry")
    } else {
        new_entry(path)?
    };
    let mut reader = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
    reader
        .seek(SeekFrom::Start(entry.scan_offset))
        .map_err(|error| error.to_string())?;
    let mut warnings = Vec::new();
    loop {
        let start = reader
            .stream_position()
            .map_err(|error| error.to_string())?;
        match read_complete_record(&mut reader)? {
            RecordRead::Eof => break,
            RecordRead::Partial => {
                reader
                    .seek(SeekFrom::Start(start))
                    .map_err(|error| error.to_string())?;
                break;
            }
            RecordRead::Oversized => add_warning(
                &mut warnings,
                format!("skip {}: rollout record at byte {start} exceeds {MAX_ROLLOUT_RECORD_BYTES} bytes", path.display()),
            ),
            RecordRead::Complete(line) => {
                if let Ok(record) = serde_json::from_slice::<Value>(&line) {
                    apply_record(&mut entry, &record);
                } else {
                    add_warning(
                        &mut warnings,
                        format!("skip {}: invalid rollout record at byte {start}", path.display()),
                    );
                }
            }
        }
        entry.scan_offset = reader
            .stream_position()
            .map_err(|error| error.to_string())?;
    }
    entry.rollout_length = length;
    entry.rollout_updated_at_unix_ms = modified;
    entry.rollout_modified_at_unix_ns = modified_nanos;
    entry.rollout_file_identity = file_identity;
    Ok(Some(RolloutUpdate {
        changed: true,
        entry,
        warnings,
    }))
}

struct RolloutUpdate {
    changed: bool,
    entry: CodexSessionCatalogEntry,
    warnings: Vec<String>,
}

impl RolloutUpdate {
    fn unchanged(entry: CodexSessionCatalogEntry) -> Self {
        Self {
            changed: false,
            entry,
            warnings: Vec::new(),
        }
    }
}

enum RecordRead {
    Eof,
    Partial,
    Complete(Vec<u8>),
    Oversized,
}

fn read_complete_record(reader: &mut BufReader<File>) -> Result<RecordRead, String> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|error| error.to_string())?;
        if buffer.is_empty() {
            return Ok(if line.is_empty() {
                RecordRead::Eof
            } else {
                RecordRead::Partial
            });
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        if line.len().saturating_add(length) > MAX_ROLLOUT_RECORD_BYTES {
            let complete = buffer[length - 1] == b'\n';
            reader.consume(length);
            if complete {
                return Ok(RecordRead::Oversized);
            }
            return Ok(if discard_record_remainder(reader)? {
                RecordRead::Oversized
            } else {
                RecordRead::Partial
            });
        }
        line.extend_from_slice(&buffer[..length]);
        let complete = line.ends_with(b"\n");
        reader.consume(length);
        if complete {
            return Ok(RecordRead::Complete(line));
        }
    }
}

fn discard_record_remainder(reader: &mut BufReader<File>) -> Result<bool, String> {
    loop {
        let buffer = reader.fill_buf().map_err(|error| error.to_string())?;
        if buffer.is_empty() {
            return Ok(false);
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        let complete = length <= buffer.len() && buffer[length - 1] == b'\n';
        reader.consume(length);
        if complete {
            return Ok(true);
        }
    }
}

fn new_entry(path: &Path) -> Result<CodexSessionCatalogEntry, String> {
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
    if !first.ends_with('\n') {
        return Err("first rollout record is incomplete".into());
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
        return Err("subagent".into());
    }
    let id = record
        .payload
        .id
        .ok_or_else(|| "session_meta has no thread ID".to_owned())?;
    let session_id = Uuid::parse_str(&id).map_err(|_| "thread ID is not a UUID".to_owned())?;
    if id != session_id.hyphenated().to_string() {
        return Err("thread ID is not canonical".into());
    }
    Ok(CodexSessionCatalogEntry {
        session_id,
        rollout_path: path.to_string_lossy().into_owned(),
        rollout_file_identity: String::new(),
        rollout_length: 0,
        scan_offset: 0,
        created_at_unix_ms: None,
        rollout_updated_at_unix_ms: 0,
        rollout_modified_at_unix_ns: 0,
        title: None,
        title_from_user_message: false,
        latest_user_message: None,
        latest_user_message_at_unix_ms: None,
        latest_agent_message: None,
        latest_agent_message_at_unix_ms: None,
        cwd: None,
        model: None,
        cli_version: None,
        turn_count: 0,
        command_count: 0,
        file_change_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
    })
}

fn apply_record(entry: &mut CodexSessionCatalogEntry, record: &Value) {
    let kind = record.get("type").and_then(Value::as_str);
    let payload = record.get("payload").unwrap_or(&Value::Null);
    match kind {
        Some("session_meta") => {
            entry.created_at_unix_ms = payload
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_timestamp)
                .or_else(|| record_timestamp(record));
            entry.cwd = bounded_metadata(payload.get("cwd"), 4096);
            entry.cli_version = bounded_metadata(payload.get("cli_version"), 64);
        }
        Some("turn_context") => {
            entry.cwd = bounded_metadata(payload.get("cwd"), 4096).or_else(|| entry.cwd.take());
            entry.model =
                bounded_metadata(payload.get("model"), 128).or_else(|| entry.model.take());
        }
        Some("event_msg") => apply_event(entry, payload, record_timestamp(record)),
        Some("response_item")
            if payload.get("type").and_then(Value::as_str) == Some("message")
                && payload.get("role").and_then(Value::as_str) == Some("user") =>
        {
            if let Some(message) = normalized_message_text(payload, "input_text") {
                if entry.title.is_none() {
                    entry.title = Some(title_from(&message));
                }
                if let Some(timestamp) = record_timestamp(record) {
                    entry.latest_user_message = Some(message);
                    entry.latest_user_message_at_unix_ms = Some(timestamp);
                }
            }
        }
        _ => {}
    }
}

fn apply_event(entry: &mut CodexSessionCatalogEntry, payload: &Value, timestamp: Option<i64>) {
    match payload.get("type").and_then(Value::as_str) {
        Some("task_started") => entry.turn_count = entry.turn_count.saturating_add(1),
        Some("token_count") => apply_tokens(entry, payload),
        Some("item_completed") => {
            let item = payload.get("item").unwrap_or(&Value::Null);
            match item.get("type").and_then(Value::as_str) {
                Some("UserMessage") => {
                    if let Some(message) = normalized_message_text(item, "text") {
                        if !entry.title_from_user_message {
                            entry.title = Some(title_from(&message));
                            entry.title_from_user_message = true;
                        }
                        if let Some(timestamp) = timestamp {
                            entry.latest_user_message = Some(message);
                            entry.latest_user_message_at_unix_ms = Some(timestamp);
                        }
                    }
                }
                Some("AgentMessage") => {
                    if let Some(message) = normalized_message_text(item, "Text") {
                        if let Some(timestamp) = timestamp {
                            entry.latest_agent_message = Some(message);
                            entry.latest_agent_message_at_unix_ms = Some(timestamp);
                        }
                    }
                }
                Some("CommandExecution") => {
                    entry.command_count = entry.command_count.saturating_add(1)
                }
                Some("FileChange") => {
                    entry.file_change_count = entry.file_change_count.saturating_add(1)
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn apply_tokens(entry: &mut CodexSessionCatalogEntry, payload: &Value) {
    let Some(total) = payload
        .get("info")
        .and_then(|info| info.get("total_token_usage"))
    else {
        return;
    };
    entry.input_tokens = unsigned(total, "input_tokens");
    entry.cached_input_tokens = unsigned(total, "cached_input_tokens");
    entry.output_tokens = unsigned(total, "output_tokens");
    entry.reasoning_output_tokens = unsigned(total, "reasoning_output_tokens");
}

fn unsigned(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or_default()
}

fn bounded_metadata(value: Option<&Value>, maximum_bytes: usize) -> Option<String> {
    let value = value?.as_str()?;
    (!value.is_empty() && !value.chars().any(char::is_control))
        .then(|| bounded_utf8(value, maximum_bytes))
}

fn record_timestamp(record: &Value) -> Option<i64> {
    record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
}

fn parse_timestamp(timestamp: &str) -> Option<i64> {
    let milliseconds = OffsetDateTime::parse(timestamp, &Rfc3339)
        .ok()?
        .unix_timestamp_nanos()
        / 1_000_000;
    i64::try_from(milliseconds).ok()
}

fn normalized_message_text(message: &Value, content_type: &str) -> Option<String> {
    let text = message
        .get("content")?
        .as_array()?
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some(content_type))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = strip_terminal_controls(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then(|| bounded_utf8(&normalized, MAX_MESSAGE_PREVIEW_BYTES))
}

fn title_from(message: &str) -> String {
    message.chars().take(160).collect()
}

fn strip_terminal_controls(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            if characters.next() == Some('[') {
                characters
                    .by_ref()
                    .find(|character| ('@'..='~').contains(character));
            }
        } else if character.is_control() {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn bounded_utf8(value: &str, maximum_bytes: usize) -> String {
    let mut end = value.len().min(maximum_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn unix_time_from(time: SystemTime) -> Result<i64, String> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
    )
    .map_err(|_| "timestamp is too large".to_owned())
}

fn unix_nanos_from(time: SystemTime) -> Result<i64, String> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos(),
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
    use std::io::Write;

    #[test]
    fn caches_session_summary_and_appended_records() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let session_id = Uuid::new_v4();
        let rollout = temp.path().join("rollout-main.jsonl");
        fs::write(
            &rollout,
            format!(
                concat!(
                    "{{\"timestamp\":\"2026-08-22T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"source\":{{}},\"timestamp\":\"2026-08-22T10:00:00Z\",\"cwd\":\"/work\",\"cli_version\":\"1.2.3\"}}}}\n",
                    "{{\"timestamp\":\"2026-08-22T10:00:30Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"Injected context\"}}]}}}}\n",
                    "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"gpt-test\",\"cwd\":\"/work/repo\"}}}}\n",
                    "{{\"timestamp\":\"2026-08-22T10:01:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n",
                    "{{\"timestamp\":\"2026-08-22T10:02:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"UserMessage\",\"content\":[{{\"type\":\"text\",\"text\":\"Build  the\\n cache\"}}]}}}}}}\n",
                    "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"CommandExecution\"}}}}}}\n",
                    "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"FileChange\"}}}}}}\n",
                    "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":1000,\"cached_input_tokens\":800,\"output_tokens\":200,\"reasoning_output_tokens\":50}}}}}}}}\n"
                ),
                session_id
            ),
        )
        .unwrap();

        assert!(refresh(&store, temp.path()).unwrap().is_empty());
        let first = store.list_codex_session_catalog().unwrap().remove(0);
        assert_eq!(first.session_id, session_id);
        assert_eq!(first.title.as_deref(), Some("Build the cache"));
        assert!(first.title_from_user_message);
        assert_eq!(first.model.as_deref(), Some("gpt-test"));
        assert_eq!(first.turn_count, 1);
        assert_eq!(first.command_count, 1);
        assert_eq!(first.file_change_count, 1);
        assert_eq!(first.input_tokens, 1_000);
        assert_eq!(first.cached_input_tokens, 800);
        assert_eq!(first.output_tokens, 200);
        assert_eq!(first.reasoning_output_tokens, 50);

        let old_offset = first.scan_offset;
        writeln!(
            fs::OpenOptions::new().append(true).open(&rollout).unwrap(),
            "{{\"timestamp\":\"2026-08-22T10:03:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"AgentMessage\",\"content\":[{{\"type\":\"Text\",\"text\":\"Cache ready\"}}]}}}}}}"
        )
        .unwrap();

        refresh(&store, temp.path()).unwrap();
        let updated = store.list_codex_session_catalog().unwrap().remove(0);
        assert!(updated.scan_offset > old_offset);
        assert_eq!(updated.turn_count, 1);
        assert_eq!(updated.latest_agent_message.as_deref(), Some("Cache ready"));
    }

    #[test]
    fn skips_subagents_and_removes_deleted_rollouts() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let rollout = temp.path().join("rollout-main.jsonl");
        fs::write(
            &rollout,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"source\":{{}}}}}}\n",
                Uuid::new_v4()
            ),
        )
        .unwrap();
        fs::write(
            temp.path().join("rollout-sub.jsonl"),
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"source\":{{\"subagent\":{{}}}}}}}}\n",
                Uuid::new_v4()
            ),
        )
        .unwrap();

        assert!(refresh(&store, temp.path()).unwrap().is_empty());
        assert_eq!(store.list_codex_session_catalog().unwrap().len(), 1);
        fs::remove_file(rollout).unwrap();
        refresh(&store, temp.path()).unwrap();
        assert!(store.list_codex_session_catalog().unwrap().is_empty());
    }

    #[test]
    fn rebuilds_when_a_rollout_path_is_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let first_id = Uuid::new_v4();
        let replacement_id = Uuid::new_v4();
        let rollout = temp.path().join("rollout-main.jsonl");
        fs::write(
            &rollout,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{first_id}\",\"source\":{{}}}}}}\n"
            ),
        )
        .unwrap();
        refresh(&store, temp.path()).unwrap();

        let replacement = temp.path().join("rollout-replacement.jsonl");
        fs::write(
            &replacement,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{replacement_id}\",\"source\":{{}}}}}}\n"
            ),
        )
        .unwrap();
        fs::rename(replacement, &rollout).unwrap();

        refresh(&store, temp.path()).unwrap();
        let sessions = store.list_codex_session_catalog().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, replacement_id);
    }

    #[test]
    fn rebuilds_when_a_rollout_is_rewritten_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let first_id = Uuid::new_v4();
        let replacement_id = Uuid::new_v4();
        let rollout = temp.path().join("rollout-main.jsonl");
        let record = |session_id| {
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"source\":{{}}}}}}\n"
            )
        };
        fs::write(&rollout, record(first_id)).unwrap();
        refresh(&store, temp.path()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1));
        fs::write(&rollout, record(replacement_id)).unwrap();
        refresh(&store, temp.path()).unwrap();

        let sessions = store.list_codex_session_catalog().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, replacement_id);
    }

    #[test]
    fn skips_oversized_records_and_continues_to_later_records() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let rollout = temp.path().join("rollout-main.jsonl");
        fs::write(
            &rollout,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"source\":{{}}}}}}\n",
                Uuid::new_v4()
            ),
        )
        .unwrap();
        let mut file = fs::OpenOptions::new().append(true).open(&rollout).unwrap();
        writeln!(file, "{}", "x".repeat(MAX_ROLLOUT_RECORD_BYTES + 1)).unwrap();
        writeln!(
            file,
            "{{\"timestamp\":\"2026-08-22T10:03:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"AgentMessage\",\"content\":[{{\"type\":\"Text\",\"text\":\"Still indexed\"}}]}}}}}}"
        )
        .unwrap();

        let warnings = refresh(&store, temp.path()).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("exceeds"));
        let session = store.list_codex_session_catalog().unwrap().remove(0);
        assert_eq!(
            session.latest_agent_message.as_deref(),
            Some("Still indexed")
        );
    }
}
