use crate::schema::{world_mail, worlds};
use crate::{Registry, RegistryError};
use diesel::dsl::max;
use diesel::prelude::*;
use wt_world::WorldId;

pub const MAX_MAIL_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAILBOX_BYTES_PER_WORLD: i64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMail {
    pub id: u64,
    pub world_id: WorldId,
    pub created_at_unix_ms: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMailPage {
    pub messages: Vec<WorldMail>,
    pub high_water_id: u64,
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
        if message.is_empty() || message.len() > MAX_MAIL_MESSAGE_BYTES {
            return Err(RegistryError::InvalidData(
                "mail message must contain 1 to 65536 UTF-8 bytes".into(),
            ));
        }
        self.immediate_transaction(|connection| {
            let retained = world_mail::table
                .filter(world_mail::world_id.eq(world_id.to_string()))
                .select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(SUM(length(CAST(message AS BLOB))), 0)",
                ))
                .first::<i64>(connection)?;
            ensure_mailbox_capacity(retained, message.len())?;
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

fn parse(row: Row) -> Result<WorldMail, RegistryError> {
    Ok(WorldMail {
        id: u64::try_from(row.id)
            .map_err(|_| RegistryError::InvalidData("invalid mail ID".into()))?,
        world_id: row
            .world_id
            .parse()
            .map_err(|error: uuid::Error| RegistryError::InvalidData(error.to_string()))?,
        created_at_unix_ms: row.created_at_unix_ms,
        message: row.message,
    })
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

fn ensure_mailbox_capacity(
    retained_bytes: i64,
    new_message_bytes: usize,
) -> Result<(), RegistryError> {
    let new_message_bytes = i64::try_from(new_message_bytes).unwrap_or(i64::MAX);
    if retained_bytes.saturating_add(new_message_bytes) > MAILBOX_BYTES_PER_WORLD {
        Err(RegistryError::MailboxCapacity)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_is_owner_scoped_cursor_read_and_cascades_with_its_world() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let alice = insert_world(&registry, "alice", "first");
        let bob = insert_world(&registry, "bob", "second");

        let first = registry.insert_world_mail(alice, "one").unwrap();
        registry.insert_world_mail(bob, "private").unwrap();
        let second = registry.insert_world_mail(alice, "two").unwrap();

        let first_page = registry.list_world_mail("alice", alice, 0, 1).unwrap();
        assert_eq!(first_page.messages, vec![first.clone()]);
        assert_eq!(first_page.high_water_id, second.id);
        let second_page = registry
            .list_world_mail("alice", alice, first.id, 1)
            .unwrap();
        assert_eq!(second_page.messages, vec![second]);
        assert_eq!(
            registry
                .list_world_mail("bob", alice, 0, 1)
                .unwrap_err()
                .to_string(),
            "resource not found"
        );

        registry
            .transaction::<_, RegistryError>(|connection| {
                diesel::delete(worlds::table.find(alice.to_string())).execute(connection)?;
                Ok(())
            })
            .unwrap();
        let remaining = registry
            .read(|connection| {
                world_mail::table
                    .filter(world_mail::world_id.eq(alice.to_string()))
                    .count()
                    .get_result::<i64>(connection)
            })
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(
            registry
                .list_world_mail("bob", bob, 0, 10)
                .unwrap()
                .messages
                .len(),
            1
        );
    }

    #[test]
    fn mail_limits_are_exact_byte_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::open(&temp.path().join("registry.db")).unwrap();
        let world_id = insert_world(&registry, "owner", "world");

        registry
            .insert_world_mail(world_id, &"x".repeat(MAX_MAIL_MESSAGE_BYTES))
            .unwrap();
        for invalid in [String::new(), "x".repeat(MAX_MAIL_MESSAGE_BYTES + 1)] {
            assert!(matches!(
                registry.insert_world_mail(world_id, &invalid),
                Err(RegistryError::InvalidData(_))
            ));
        }
        assert!(ensure_mailbox_capacity(
            MAILBOX_BYTES_PER_WORLD - MAX_MAIL_MESSAGE_BYTES as i64,
            MAX_MAIL_MESSAGE_BYTES
        )
        .is_ok());
        assert!(matches!(
            ensure_mailbox_capacity(MAILBOX_BYTES_PER_WORLD, 1),
            Err(RegistryError::MailboxCapacity)
        ));
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
