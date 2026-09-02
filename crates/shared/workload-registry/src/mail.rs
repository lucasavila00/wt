use crate::schema::{windows, world_mail, worlds};
use crate::{Registry, RegistryError};
use diesel::dsl::max;
use diesel::prelude::*;
use std::collections::BTreeMap;
use wt_world::{WindowId, WorldId};

pub const MAX_MAIL_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAILBOX_BYTES_PER_WORLD: i64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMail {
    pub id: u64,
    pub client_message_id: uuid::Uuid,
    pub world_id: WorldId,
    pub world_name: String,
    pub window_id: WindowId,
    pub created_at_unix_ms: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMailPage {
    pub messages: Vec<WorldMail>,
    pub high_water_id: u64,
}

#[derive(Insertable)]
#[diesel(table_name = world_mail)]
struct NewWorldMail<'a> {
    client_message_id: String,
    world_id: String,
    window_id: String,
    created_at_unix_ms: i64,
    message: &'a str,
}

#[derive(Queryable)]
struct WorldMailRow {
    id: i64,
    client_message_id: String,
    world_id: String,
    world_name: String,
    window_id: String,
    created_at_unix_ms: i64,
    message: String,
}

impl Registry {
    pub fn insert_world_mail(
        &self,
        world_id: WorldId,
        window_id: WindowId,
        client_message_id: uuid::Uuid,
        message: &str,
    ) -> Result<WorldMail, RegistryError> {
        validate_message(message)?;
        self.immediate_transaction(|connection| {
            let existing = world_mail::table
                .inner_join(worlds::table)
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .filter(world_mail::window_id.eq(window_id.to_string()))
                .filter(world_mail::client_message_id.eq(client_message_id.to_string()))
                .select(mail_selection())
                .first::<WorldMailRow>(connection)
                .optional()?;
            if let Some(existing) = existing {
                return parse_mail(existing);
            }
            let managed = windows::table
                .find(window_id.to_string())
                .filter(windows::world_id.eq(world_id.to_string()))
                .select(windows::window_id)
                .first::<String>(connection)
                .optional()?;
            if managed.is_none() {
                return Err(RegistryError::NotFound);
            }
            let retained = world_mail::table
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(SUM(length(CAST(message AS BLOB))), 0)",
                ))
                .first::<i64>(connection)?;
            let message_bytes = i64::try_from(message.len()).map_err(|_| {
                RegistryError::InvalidData("mail message byte length is too large".into())
            })?;
            if !mailbox_has_capacity(retained, message_bytes) {
                return Err(RegistryError::MailboxCapacity);
            }
            diesel::insert_into(world_mail::table)
                .values(NewWorldMail {
                    client_message_id: client_message_id.to_string(),
                    world_id: world_id.to_string(),
                    window_id: window_id.to_string(),
                    created_at_unix_ms: now_unix_ms()?,
                    message,
                })
                .execute(connection)?;
            let row = world_mail::table
                .inner_join(worlds::table)
                .order(world_mail::id.desc())
                .select(mail_selection())
                .first::<WorldMailRow>(connection)?;
            parse_mail(row)
        })
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
            let owned = worlds::table
                .find(world_id.to_string())
                .filter(worlds::owner.eq(owner))
                .select(worlds::world_id)
                .first::<String>(connection)
                .optional()?;
            if owned.is_none() {
                return Err(RegistryError::NotFound);
            }
            let high_water_id = world_mail::table
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .select(max(world_mail::id))
                .first::<Option<i64>>(connection)?
                .unwrap_or_default();
            let rows = world_mail::table
                .inner_join(worlds::table)
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .filter(world_mail::id.gt(after_id))
                .filter(world_mail::id.le(high_water_id))
                .order(world_mail::id.asc())
                .limit(i64::from(limit))
                .select(mail_selection())
                .load::<WorldMailRow>(connection)?;
            Ok(WorldMailPage {
                messages: rows.into_iter().map(parse_mail).collect::<Result<_, _>>()?,
                high_water_id: row_id(high_water_id)?,
            })
        })
    }

    pub fn world_mail_counts(&self, owner: &str) -> Result<BTreeMap<WorldId, u64>, RegistryError> {
        self.read(|connection| {
            world_mail::table
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .group_by(world_mail::world_id)
                .select((world_mail::world_id, diesel::dsl::count_star()))
                .load::<(String, i64)>(connection)?
                .into_iter()
                .map(|(world_id, count)| {
                    Ok((
                        world_id.parse().map_err(|error: uuid::Error| {
                            RegistryError::InvalidData(error.to_string())
                        })?,
                        row_id(count)?,
                    ))
                })
                .collect()
        })
    }
}

fn mail_selection() -> (
    world_mail::id,
    world_mail::client_message_id,
    world_mail::world_id,
    worlds::name,
    world_mail::window_id,
    world_mail::created_at_unix_ms,
    world_mail::message,
) {
    (
        world_mail::id,
        world_mail::client_message_id,
        world_mail::world_id,
        worlds::name,
        world_mail::window_id,
        world_mail::created_at_unix_ms,
        world_mail::message,
    )
}

fn parse_mail(row: WorldMailRow) -> Result<WorldMail, RegistryError> {
    Ok(WorldMail {
        id: row_id(row.id)?,
        client_message_id: row
            .client_message_id
            .parse()
            .map_err(|error: uuid::Error| RegistryError::InvalidData(error.to_string()))?,
        world_id: row
            .world_id
            .parse()
            .map_err(|error: uuid::Error| RegistryError::InvalidData(error.to_string()))?,
        world_name: row.world_name,
        window_id: row
            .window_id
            .parse()
            .map_err(|error: uuid::Error| RegistryError::InvalidData(error.to_string()))?,
        created_at_unix_ms: row.created_at_unix_ms,
        message: row.message,
    })
}

fn row_id(id: i64) -> Result<u64, RegistryError> {
    u64::try_from(id).map_err(|_| RegistryError::InvalidData("invalid mail ID".into()))
}

fn validate_message(message: &str) -> Result<(), RegistryError> {
    if message.is_empty() {
        return Err(RegistryError::InvalidData("mail message is empty".into()));
    }
    if message.len() > MAX_MAIL_MESSAGE_BYTES {
        return Err(RegistryError::InvalidData(format!(
            "mail message exceeds {MAX_MAIL_MESSAGE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn mailbox_has_capacity(retained_bytes: i64, message_bytes: i64) -> bool {
    retained_bytes.saturating_add(message_bytes) <= MAILBOX_BYTES_PER_WORLD
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
    use crate::schema::worlds;

    #[test]
    fn mail_is_idempotent_cursor_ordered_owner_scoped_and_cascaded() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let first = insert_world(&registry, "alice", "first");
        let second = insert_world(&registry, "bob", "second");
        let window = WindowId::new();
        insert_window(&registry, first, window);
        let second_window = WindowId::new();
        insert_window(&registry, second, second_window);
        let request = uuid::Uuid::new_v4();
        let inserted = registry
            .insert_world_mail(first, window, request, "done")
            .unwrap();
        let replay = registry
            .insert_world_mail(first, window, request, "done")
            .unwrap();
        assert_eq!(replay, inserted);
        registry
            .insert_world_mail(first, window, uuid::Uuid::new_v4(), "next")
            .unwrap();
        registry
            .insert_world_mail(second, second_window, uuid::Uuid::new_v4(), "hidden")
            .unwrap();
        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::delete(windows::table.find(window.to_string())).execute(connection)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            registry
                .insert_world_mail(first, window, request, "done")
                .unwrap(),
            inserted
        );

        let page = registry.list_world_mail("alice", first, 0, 1).unwrap();
        assert_eq!(page.messages, vec![inserted.clone()]);
        assert!(page.high_water_id > inserted.id);
        let rest = registry
            .list_world_mail("alice", first, inserted.id, 10)
            .unwrap();
        assert_eq!(rest.messages.len(), 1);
        assert!(matches!(
            registry.list_world_mail("bob", first, 0, 10),
            Err(RegistryError::NotFound)
        ));
        assert_eq!(registry.world_mail_counts("alice").unwrap()[&first], 2);

        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::delete(worlds::table.find(first.to_string())).execute(connection)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            registry
                .read(|connection| world_mail::table.count().get_result::<i64>(connection))
                .unwrap(),
            1
        );
    }

    #[test]
    fn rejects_empty_and_oversized_mail() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let world = insert_world(&registry, "alice", "first");
        let window = WindowId::new();
        insert_window(&registry, world, window);
        for message in [String::new(), "x".repeat(MAX_MAIL_MESSAGE_BYTES + 1)] {
            assert!(registry
                .insert_world_mail(world, window, uuid::Uuid::new_v4(), &message)
                .is_err());
        }
    }

    #[test]
    fn mailbox_capacity_includes_the_exact_limit() {
        assert!(mailbox_has_capacity(MAILBOX_BYTES_PER_WORLD - 1, 1));
        assert!(!mailbox_has_capacity(MAILBOX_BYTES_PER_WORLD, 1));
    }

    #[test]
    fn rejects_unknown_and_cross_world_windows() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let first = insert_world(&registry, "alice", "first");
        let second = insert_world(&registry, "alice", "second");
        let window = WindowId::new();
        insert_window(&registry, second, window);

        for invalid in [window, WindowId::new()] {
            assert!(matches!(
                registry.insert_world_mail(first, invalid, uuid::Uuid::new_v4(), "message"),
                Err(RegistryError::NotFound)
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

    fn insert_window(registry: &Registry, world_id: WorldId, window_id: WindowId) {
        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::insert_into(windows::table)
                    .values((
                        windows::window_id.eq(window_id.to_string()),
                        windows::world_id.eq(world_id.to_string()),
                        windows::owner.eq("alice"),
                        windows::tmux_window_id.eq(format!("@{window_id}")),
                        windows::control_token.eq("token"),
                        windows::control_token_hash.eq("hash"),
                        windows::argv_json.eq("[]"),
                        windows::cwd.eq("/home/wt"),
                        windows::state.eq("running"),
                        windows::created_at_unix_ms.eq(0_i64),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .unwrap();
    }
}
