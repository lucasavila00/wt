use crate::schema::codex_session_catalog;
use crate::{Registry, RegistryError};
use diesel::prelude::*;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSessionCatalogEntry {
    pub session_id: Uuid,
    pub rollout_path: String,
    pub rollout_file_identity: String,
    pub rollout_length: u64,
    pub scan_offset: u64,
    pub created_at_unix_ms: Option<i64>,
    pub rollout_updated_at_unix_ms: i64,
    pub rollout_modified_at_unix_ns: i64,
    pub title: Option<String>,
    pub title_from_user_message: bool,
    pub latest_user_message: Option<String>,
    pub latest_user_message_at_unix_ms: Option<i64>,
    pub latest_agent_message: Option<String>,
    pub latest_agent_message_at_unix_ms: Option<i64>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub cli_version: Option<String>,
    pub turn_count: u64,
    pub command_count: u64,
    pub file_change_count: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

pub type CodexSessionCatalogInput = CodexSessionCatalogEntry;

#[derive(Insertable)]
#[diesel(table_name = codex_session_catalog)]
struct CatalogRow<'a> {
    session_id: String,
    rollout_path: &'a str,
    rollout_file_identity: &'a str,
    rollout_length: i64,
    scan_offset: i64,
    created_at_unix_ms: Option<i64>,
    rollout_updated_at_unix_ms: i64,
    rollout_modified_at_unix_ns: i64,
    title: Option<&'a str>,
    title_from_user_message: bool,
    latest_user_message: Option<&'a str>,
    latest_user_message_at_unix_ms: Option<i64>,
    latest_agent_message: Option<&'a str>,
    latest_agent_message_at_unix_ms: Option<i64>,
    cwd: Option<&'a str>,
    model: Option<&'a str>,
    cli_version: Option<&'a str>,
    turn_count: i64,
    command_count: i64,
    file_change_count: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = codex_session_catalog)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct StoredCatalogRow {
    session_id: String,
    rollout_path: String,
    rollout_file_identity: String,
    rollout_length: i64,
    scan_offset: i64,
    created_at_unix_ms: Option<i64>,
    rollout_updated_at_unix_ms: i64,
    rollout_modified_at_unix_ns: i64,
    title: Option<String>,
    title_from_user_message: bool,
    latest_user_message: Option<String>,
    latest_user_message_at_unix_ms: Option<i64>,
    latest_agent_message: Option<String>,
    latest_agent_message_at_unix_ms: Option<i64>,
    cwd: Option<String>,
    model: Option<String>,
    cli_version: Option<String>,
    turn_count: i64,
    command_count: i64,
    file_change_count: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
}

impl Registry {
    pub fn upsert_codex_session_catalog(
        &self,
        entry: &CodexSessionCatalogInput,
    ) -> Result<(), RegistryError> {
        let row = CatalogRow::try_from(entry)?;
        self.immediate_transaction(|connection| {
            diesel::delete(
                codex_session_catalog::table
                    .filter(codex_session_catalog::rollout_path.eq(row.rollout_path))
                    .filter(codex_session_catalog::session_id.ne(&row.session_id)),
            )
            .execute(connection)?;
            diesel::insert_into(codex_session_catalog::table)
                .values(&row)
                .on_conflict(codex_session_catalog::session_id)
                .do_update()
                .set((
                    codex_session_catalog::rollout_path.eq(row.rollout_path),
                    codex_session_catalog::rollout_file_identity.eq(row.rollout_file_identity),
                    codex_session_catalog::rollout_length.eq(row.rollout_length),
                    codex_session_catalog::scan_offset.eq(row.scan_offset),
                    codex_session_catalog::created_at_unix_ms.eq(row.created_at_unix_ms),
                    codex_session_catalog::rollout_updated_at_unix_ms
                        .eq(row.rollout_updated_at_unix_ms),
                    codex_session_catalog::rollout_modified_at_unix_ns
                        .eq(row.rollout_modified_at_unix_ns),
                    codex_session_catalog::title.eq(row.title),
                    codex_session_catalog::title_from_user_message.eq(row.title_from_user_message),
                    codex_session_catalog::latest_user_message.eq(row.latest_user_message),
                    codex_session_catalog::latest_user_message_at_unix_ms
                        .eq(row.latest_user_message_at_unix_ms),
                    codex_session_catalog::latest_agent_message.eq(row.latest_agent_message),
                    codex_session_catalog::latest_agent_message_at_unix_ms
                        .eq(row.latest_agent_message_at_unix_ms),
                    codex_session_catalog::cwd.eq(row.cwd),
                    codex_session_catalog::model.eq(row.model),
                    codex_session_catalog::cli_version.eq(row.cli_version),
                    codex_session_catalog::turn_count.eq(row.turn_count),
                    codex_session_catalog::command_count.eq(row.command_count),
                    codex_session_catalog::file_change_count.eq(row.file_change_count),
                    codex_session_catalog::input_tokens.eq(row.input_tokens),
                    codex_session_catalog::cached_input_tokens.eq(row.cached_input_tokens),
                    codex_session_catalog::output_tokens.eq(row.output_tokens),
                    codex_session_catalog::reasoning_output_tokens.eq(row.reasoning_output_tokens),
                ))
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn list_codex_session_catalog(
        &self,
    ) -> Result<Vec<CodexSessionCatalogEntry>, RegistryError> {
        self.read(|connection| {
            codex_session_catalog::table
                .select(StoredCatalogRow::as_select())
                .load(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }

    pub fn retain_codex_session_catalog_paths(
        &self,
        paths: &BTreeSet<String>,
    ) -> Result<(), RegistryError> {
        self.immediate_transaction(|connection| {
            let stored = codex_session_catalog::table
                .select(codex_session_catalog::rollout_path)
                .load::<String>(connection)?;
            for path in stored.into_iter().filter(|path| !paths.contains(path)) {
                diesel::delete(
                    codex_session_catalog::table
                        .filter(codex_session_catalog::rollout_path.eq(path)),
                )
                .execute(connection)?;
            }
            Ok(())
        })
    }
}

impl<'a> TryFrom<&'a CodexSessionCatalogEntry> for CatalogRow<'a> {
    type Error = RegistryError;

    fn try_from(value: &'a CodexSessionCatalogEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: value.session_id.to_string(),
            rollout_path: &value.rollout_path,
            rollout_file_identity: &value.rollout_file_identity,
            rollout_length: number(value.rollout_length, "rollout length")?,
            scan_offset: number(value.scan_offset, "scan offset")?,
            created_at_unix_ms: value.created_at_unix_ms,
            rollout_updated_at_unix_ms: value.rollout_updated_at_unix_ms,
            rollout_modified_at_unix_ns: value.rollout_modified_at_unix_ns,
            title: value.title.as_deref(),
            title_from_user_message: value.title_from_user_message,
            latest_user_message: value.latest_user_message.as_deref(),
            latest_user_message_at_unix_ms: value.latest_user_message_at_unix_ms,
            latest_agent_message: value.latest_agent_message.as_deref(),
            latest_agent_message_at_unix_ms: value.latest_agent_message_at_unix_ms,
            cwd: value.cwd.as_deref(),
            model: value.model.as_deref(),
            cli_version: value.cli_version.as_deref(),
            turn_count: number(value.turn_count, "turn count")?,
            command_count: number(value.command_count, "command count")?,
            file_change_count: number(value.file_change_count, "file change count")?,
            input_tokens: number(value.input_tokens, "input tokens")?,
            cached_input_tokens: number(value.cached_input_tokens, "cached input tokens")?,
            output_tokens: number(value.output_tokens, "output tokens")?,
            reasoning_output_tokens: number(
                value.reasoning_output_tokens,
                "reasoning output tokens",
            )?,
        })
    }
}

impl TryFrom<StoredCatalogRow> for CodexSessionCatalogEntry {
    type Error = RegistryError;

    fn try_from(row: StoredCatalogRow) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: Uuid::parse_str(&row.session_id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            rollout_path: row.rollout_path,
            rollout_file_identity: row.rollout_file_identity,
            rollout_length: unsigned(row.rollout_length, "rollout length")?,
            scan_offset: unsigned(row.scan_offset, "scan offset")?,
            created_at_unix_ms: row.created_at_unix_ms,
            rollout_updated_at_unix_ms: row.rollout_updated_at_unix_ms,
            rollout_modified_at_unix_ns: row.rollout_modified_at_unix_ns,
            title: row.title,
            title_from_user_message: row.title_from_user_message,
            latest_user_message: row.latest_user_message,
            latest_user_message_at_unix_ms: row.latest_user_message_at_unix_ms,
            latest_agent_message: row.latest_agent_message,
            latest_agent_message_at_unix_ms: row.latest_agent_message_at_unix_ms,
            cwd: row.cwd,
            model: row.model,
            cli_version: row.cli_version,
            turn_count: unsigned(row.turn_count, "turn count")?,
            command_count: unsigned(row.command_count, "command count")?,
            file_change_count: unsigned(row.file_change_count, "file change count")?,
            input_tokens: unsigned(row.input_tokens, "input tokens")?,
            cached_input_tokens: unsigned(row.cached_input_tokens, "cached input tokens")?,
            output_tokens: unsigned(row.output_tokens, "output tokens")?,
            reasoning_output_tokens: unsigned(
                row.reasoning_output_tokens,
                "reasoning output tokens",
            )?,
        })
    }
}

fn number(value: u64, field: &'static str) -> Result<i64, RegistryError> {
    i64::try_from(value).map_err(|_| RegistryError::InvalidData(format!("invalid {field}")))
}

fn unsigned(value: i64, field: &'static str) -> Result<u64, RegistryError> {
    u64::try_from(value).map_err(|_| RegistryError::InvalidData(format!("invalid {field}")))
}
