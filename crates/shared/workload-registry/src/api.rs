use crate::schema::{api_mutation_results, server_metadata};
use crate::{Store, StoreError};
use diesel::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const RESULT_RETENTION_MILLIS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Eq, PartialEq)]
pub enum ApiMutationStart {
    Started {
        expires_at_unix_ms: i64,
    },
    Replay {
        response_json: String,
        expires_at_unix_ms: i64,
    },
    InProgress,
    Conflict,
}

#[derive(Insertable)]
#[diesel(table_name = api_mutation_results)]
struct NewApiMutation<'a> {
    owner: &'a str,
    request_id: String,
    request_hash: &'a str,
    response_json: Option<&'a str>,
    expires_at_unix_ms: i64,
    preserve_on_restart: bool,
}

#[derive(Queryable)]
struct ApiMutationRow {
    request_hash: String,
    response_json: Option<String>,
    expires_at_unix_ms: i64,
}

impl Store {
    pub fn server_id(&self) -> Result<Uuid, StoreError> {
        self.registry.read(|connection| {
            let generated = Uuid::new_v4().to_string();
            diesel::insert_or_ignore_into(server_metadata::table)
                .values((
                    server_metadata::singleton.eq(1),
                    server_metadata::server_id.eq(&generated),
                ))
                .execute(connection)?;
            let value = server_metadata::table
                .find(1)
                .select(server_metadata::server_id)
                .first::<String>(connection)?;
            value
                .parse()
                .map_err(|error: uuid::Error| StoreError::InvalidData(error.to_string()))
        })
    }

    pub fn begin_api_mutation(
        &self,
        owner: &str,
        request_id: Uuid,
        request_hash: &str,
        preserve_on_restart: bool,
    ) -> Result<ApiMutationStart, StoreError> {
        self.begin_api_mutation_at(
            owner,
            request_id,
            request_hash,
            now_unix_ms()?,
            preserve_on_restart,
        )
    }

    fn begin_api_mutation_at(
        &self,
        owner: &str,
        request_id: Uuid,
        request_hash: &str,
        now_unix_ms: i64,
        preserve_on_restart: bool,
    ) -> Result<ApiMutationStart, StoreError> {
        let expires_at_unix_ms = now_unix_ms
            .checked_add(RESULT_RETENTION_MILLIS)
            .ok_or_else(|| StoreError::InvalidData("API result expiration overflow".into()))?;
        self.registry.immediate_transaction(|connection| {
            diesel::delete(
                api_mutation_results::table
                    .filter(api_mutation_results::expires_at_unix_ms.le(now_unix_ms)),
            )
            .execute(connection)?;
            let existing = api_mutation_results::table
                .filter(api_mutation_results::owner.eq(owner))
                .filter(api_mutation_results::request_id.eq(request_id.to_string()))
                .select((
                    api_mutation_results::request_hash,
                    api_mutation_results::response_json,
                    api_mutation_results::expires_at_unix_ms,
                ))
                .first::<ApiMutationRow>(connection)
                .optional()?;
            if let Some(existing) = existing {
                if existing.request_hash != request_hash {
                    return Ok(ApiMutationStart::Conflict);
                }
                return Ok(match existing.response_json {
                    Some(response_json) => ApiMutationStart::Replay {
                        response_json,
                        expires_at_unix_ms: existing.expires_at_unix_ms,
                    },
                    None => ApiMutationStart::InProgress,
                });
            }
            diesel::insert_into(api_mutation_results::table)
                .values(NewApiMutation {
                    owner,
                    request_id: request_id.to_string(),
                    request_hash,
                    response_json: None,
                    expires_at_unix_ms,
                    preserve_on_restart,
                })
                .execute(connection)?;
            Ok(ApiMutationStart::Started { expires_at_unix_ms })
        })
    }

    pub fn finish_api_mutation(
        &self,
        owner: &str,
        request_id: Uuid,
        request_hash: &str,
        response_json: &str,
    ) -> Result<(), StoreError> {
        self.registry.read(|connection| {
            let changed = diesel::update(
                api_mutation_results::table
                    .filter(api_mutation_results::owner.eq(owner))
                    .filter(api_mutation_results::request_id.eq(request_id.to_string()))
                    .filter(api_mutation_results::request_hash.eq(request_hash))
                    .filter(api_mutation_results::response_json.is_null()),
            )
            .set(api_mutation_results::response_json.eq(response_json))
            .execute(connection)?;
            if changed == 1 {
                Ok(())
            } else {
                Err(StoreError::InvalidData(
                    "API mutation reservation is missing".into(),
                ))
            }
        })
    }

    pub fn abort_api_mutation(
        &self,
        owner: &str,
        request_id: Uuid,
        request_hash: &str,
    ) -> Result<(), StoreError> {
        self.registry.read(|connection| {
            diesel::delete(
                api_mutation_results::table
                    .filter(api_mutation_results::owner.eq(owner))
                    .filter(api_mutation_results::request_id.eq(request_id.to_string()))
                    .filter(api_mutation_results::request_hash.eq(request_hash))
                    .filter(api_mutation_results::response_json.is_null()),
            )
            .execute(connection)?;
            Ok(())
        })
    }

    pub fn clear_incomplete_api_mutations(&self) -> Result<(), StoreError> {
        self.registry.read(|connection| {
            diesel::delete(
                api_mutation_results::table
                    .filter(api_mutation_results::response_json.is_null())
                    .filter(api_mutation_results::preserve_on_restart.eq(false)),
            )
            .execute(connection)?;
            Ok(())
        })
    }
}

fn now_unix_ms() -> Result<i64, StoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::InvalidData("system time is before Unix epoch".into()))?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| StoreError::InvalidData("system time is too large".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_identity_is_persistent() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("instances.db");
        let first = Store::open(&path).unwrap().server_id().unwrap();
        let second = Store::open(&path).unwrap().server_id().unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn mutation_results_replay_and_request_ids_cannot_change_meaning() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let request_id = Uuid::new_v4();

        assert!(matches!(
            store
                .begin_api_mutation_at("owner", request_id, r#"{"operation":"create"}"#, 100, false)
                .unwrap(),
            ApiMutationStart::Started { .. }
        ));
        assert_eq!(
            store
                .begin_api_mutation_at("owner", request_id, r#"{"operation":"create"}"#, 100, false)
                .unwrap(),
            ApiMutationStart::InProgress
        );
        store
            .finish_api_mutation(
                "owner",
                request_id,
                r#"{"operation":"create"}"#,
                r#"{"outcome":"ok"}"#,
            )
            .unwrap();
        assert!(matches!(
            store
                .begin_api_mutation_at("owner", request_id, r#"{"operation":"create"}"#, 100, false)
                .unwrap(),
            ApiMutationStart::Replay { response_json, .. }
                if response_json == r#"{"outcome":"ok"}"#
        ));
        assert_eq!(
            store
                .begin_api_mutation_at("owner", request_id, r#"{"operation":"delete"}"#, 100, false)
                .unwrap(),
            ApiMutationStart::Conflict
        );
    }

    #[test]
    fn expired_and_interrupted_mutations_can_start_again() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        let expired = Uuid::new_v4();
        let interrupted = Uuid::new_v4();
        let request = r#"{"operation":"delete"}"#;

        store
            .begin_api_mutation_at("owner", expired, request, 100, false)
            .unwrap();
        assert!(matches!(
            store
                .begin_api_mutation_at(
                    "owner",
                    expired,
                    request,
                    100 + RESULT_RETENTION_MILLIS,
                    false
                )
                .unwrap(),
            ApiMutationStart::Started { .. }
        ));
        store
            .begin_api_mutation_at("owner", interrupted, request, 100, false)
            .unwrap();
        store.clear_incomplete_api_mutations().unwrap();
        assert!(matches!(
            store
                .begin_api_mutation_at("owner", interrupted, request, 100, false)
                .unwrap(),
            ApiMutationStart::Started { .. }
        ));
    }
}
