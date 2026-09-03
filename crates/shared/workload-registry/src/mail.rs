use crate::schema::{world_mail, worlds};
use crate::{Registry, RegistryError};
use diesel::dsl::max;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use wt_world::WorldId;

pub const MAX_MAIL_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const CODEX_WINDOW_RESULT_PREFIX: &str = "WT_CODEX_RESULT_V2:";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailKind {
    Message,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMail {
    pub id: u64,
    pub world_id: WorldId,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub pane_id: Option<String>,
    pub created_at_unix_ms: i64,
    pub kind: MailKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMailPage {
    pub messages: Vec<WorldMail>,
    pub high_water_id: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CodexWindowResultEnvelope<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    pane_id: &'a str,
    kind: MailKind,
    #[serde(borrow)]
    message: std::borrow::Cow<'a, str>,
}

#[derive(Queryable)]
struct Row {
    id: i64,
    world_id: String,
    created_at_unix_ms: i64,
    message: String,
}

impl Registry {
    pub fn insert_world_mail(
        &self,
        world_id: WorldId,
        message: &str,
    ) -> Result<WorldMail, RegistryError> {
        validate_mail_text(message)?;
        self.immediate_transaction(|connection| insert_mail_row(connection, world_id, message))
    }

    pub fn insert_codex_result(
        &self,
        world_id: WorldId,
        thread_id: &str,
        turn_id: &str,
        pane_id: &str,
        kind: MailKind,
        message: &str,
    ) -> Result<WorldMail, RegistryError> {
        validate_mail_text(message)?;
        if !matches!(kind, MailKind::Completed | MailKind::Failed) {
            return Err(RegistryError::InvalidData(
                "Codex result must be completed or failed".into(),
            ));
        }
        if thread_id.is_empty() || turn_id.is_empty() || pane_id.is_empty() {
            return Err(RegistryError::InvalidData(
                "Codex result identity must not be empty".into(),
            ));
        }
        let envelope = serde_json::to_string(&CodexWindowResultEnvelope {
            thread_id,
            turn_id,
            pane_id,
            kind,
            message: message.into(),
        })
        .map_err(|error| RegistryError::InvalidData(error.to_string()))?;
        let stored = format!("{CODEX_WINDOW_RESULT_PREFIX}{envelope}");
        self.immediate_transaction(|connection| insert_mail_row(connection, world_id, &stored))
    }

    pub fn list_world_mail(
        &self,
        owner: &str,
        world_id: WorldId,
        after_id: u64,
        limit: u32,
    ) -> Result<WorldMailPage, RegistryError> {
        let after_id = i64::try_from(after_id)
            .map_err(|_| RegistryError::InvalidData("mail cursor is too large".into()))?;
        self.transaction(|connection| {
            if worlds::table
                .find(world_id.to_string())
                .filter(worlds::owner.eq(owner))
                .select(worlds::world_id)
                .first::<String>(connection)
                .optional()?
                .is_none()
            {
                return Err(RegistryError::NotFound);
            }
            let high_water_id = world_mail::table
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .select(max(world_mail::id))
                .first::<Option<i64>>(connection)?
                .unwrap_or_default();
            let rows = world_mail::table
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .filter(world_mail::id.gt(after_id))
                .filter(world_mail::id.le(high_water_id))
                .order(world_mail::id.asc())
                .limit(i64::from(limit))
                .select((
                    world_mail::id,
                    world_mail::world_id,
                    world_mail::created_at_unix_ms,
                    world_mail::message,
                ))
                .load(connection)?;
            Ok(WorldMailPage {
                messages: rows.into_iter().map(parse).collect::<Result<_, _>>()?,
                high_water_id: u64::try_from(high_water_id)
                    .map_err(|_| RegistryError::InvalidData("invalid mail ID".into()))?,
            })
        })
    }
}

fn validate_mail_text(message: &str) -> Result<(), RegistryError> {
    if message.is_empty() || message.len() > MAX_MAIL_MESSAGE_BYTES {
        return Err(RegistryError::InvalidData(format!(
            "mail message must contain 1 to {MAX_MAIL_MESSAGE_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn insert_mail_row(
    connection: &mut diesel::sqlite::SqliteConnection,
    world_id: WorldId,
    message: &str,
) -> Result<WorldMail, RegistryError> {
    diesel::insert_into(world_mail::table)
        .values((
            world_mail::world_id.eq(world_id.to_string()),
            world_mail::created_at_unix_ms.eq(now_unix_ms()?),
            world_mail::message.eq(message),
        ))
        .execute(connection)?;
    let row = world_mail::table
        .order(world_mail::id.desc())
        .select((
            world_mail::id,
            world_mail::world_id,
            world_mail::created_at_unix_ms,
            world_mail::message,
        ))
        .first::<Row>(connection)?;
    parse(row)
}

fn parse(row: Row) -> Result<WorldMail, RegistryError> {
    let (thread_id, turn_id, pane_id, kind, message) = decode(&row.message);
    Ok(WorldMail {
        id: u64::try_from(row.id)
            .map_err(|_| RegistryError::InvalidData("invalid mail ID".into()))?,
        world_id: row
            .world_id
            .parse()
            .map_err(|error: uuid::Error| RegistryError::InvalidData(error.to_string()))?,
        thread_id,
        turn_id,
        pane_id,
        created_at_unix_ms: row.created_at_unix_ms,
        kind,
        message,
    })
}

fn decode(
    stored: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    MailKind,
    String,
) {
    if let Some(json) = stored.strip_prefix(CODEX_WINDOW_RESULT_PREFIX) {
        return match serde_json::from_str::<CodexWindowResultEnvelope<'_>>(json) {
            Ok(envelope) if matches!(envelope.kind, MailKind::Completed | MailKind::Failed) => (
                Some(envelope.thread_id.to_owned()),
                Some(envelope.turn_id.to_owned()),
                Some(envelope.pane_id.to_owned()),
                envelope.kind,
                envelope.message.into_owned(),
            ),
            _ => (None, None, None, MailKind::Message, stored.to_owned()),
        };
    }
    (None, None, None, MailKind::Message, stored.to_owned())
}

fn now_unix_ms() -> Result<i64, RegistryError> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| RegistryError::InvalidData(error.to_string()))?
            .as_millis(),
    )
    .map_err(|_| RegistryError::InvalidData("system time is too large".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_codex_mail_are_read_and_world_deletion_cascades() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let world_id = insert_world(&registry, "alice", "first");
        registry.insert_world_mail(world_id, "hello").unwrap();
        registry
            .insert_codex_result(
                world_id,
                "thread-1",
                "turn-1",
                "%7",
                MailKind::Completed,
                "done",
            )
            .unwrap();
        let page = registry.list_world_mail("alice", world_id, 0, 10).unwrap();
        assert_eq!(page.messages[0].kind, MailKind::Message);
        assert_eq!(page.messages[0].message, "hello");
        assert_eq!(page.messages[1].thread_id.as_deref(), Some("thread-1"));
        assert_eq!(page.messages[1].turn_id.as_deref(), Some("turn-1"));
        assert_eq!(page.messages[1].pane_id.as_deref(), Some("%7"));
        assert_eq!(page.messages[1].kind, MailKind::Completed);
        assert_eq!(page.messages[1].message, "done");
        assert!(matches!(
            registry.list_world_mail("bob", world_id, 0, 10),
            Err(RegistryError::NotFound)
        ));
        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::delete(worlds::table.find(world_id.to_string())).execute(connection)?;
                Ok(())
            })
            .unwrap();
        let count = registry
            .read(|connection| world_mail::table.count().get_result::<i64>(connection))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn mail_limit_is_a_coarse_per_message_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let world_id = insert_world(&registry, "owner", "world");
        registry
            .insert_world_mail(world_id, &"x".repeat(MAX_MAIL_MESSAGE_BYTES))
            .unwrap();
        assert!(registry
            .insert_world_mail(world_id, "another message")
            .is_ok());
        for invalid in [String::new(), "x".repeat(MAX_MAIL_MESSAGE_BYTES + 1)] {
            assert!(matches!(
                registry.insert_world_mail(world_id, &invalid),
                Err(RegistryError::InvalidData(_))
            ));
        }
    }

    fn insert_world(registry: &Registry, owner: &str, name: &str) -> WorldId {
        let world_id = WorldId::new();
        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::insert_into(worlds::table)
                    .values((
                        worlds::world_id.eq(world_id.to_string()),
                        worlds::vcpus.eq(1_i64),
                        worlds::memory_mib.eq(1024_i64),
                        worlds::disk_gib.eq(10_i64),
                        worlds::compute_reserved.eq(true),
                        worlds::disk_reserved_gib.eq(10_i64),
                        worlds::owner.eq(owner),
                        worlds::name.eq(name),
                        worlds::status.eq("running"),
                        worlds::setup_fingerprint.eq("fingerprint"),
                        worlds::ssh_host_keys.eq("[]"),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .unwrap();
        world_id
    }
}
