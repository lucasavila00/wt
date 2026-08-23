use crate::activity::{
    intern_repository, validate_target, GitActivity, GitActivityQuery, RepositoryTargetInput,
    WtToolsActivity, WtToolsActivityQuery,
};
use crate::schema::{codex_checkout_state, codex_session_reports, repositories, worlds};
use crate::{Registry, RegistryError};
use diesel::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexSessionState {
    Unknown,
    Working,
    NeedsAttention,
    Inactive,
}

impl CodexSessionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Working => "working",
            Self::NeedsAttention => "needs_attention",
            Self::Inactive => "inactive",
        }
    }

    fn parse(value: &str) -> Result<Self, RegistryError> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "working" => Ok(Self::Working),
            "needs_attention" => Ok(Self::NeedsAttention),
            "inactive" => Ok(Self::Inactive),
            _ => Err(RegistryError::InvalidData(format!(
                "invalid Codex session state: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSessionReport {
    pub world_id: Uuid,
    pub world_name: String,
    pub session_id: Uuid,
    pub cwd: String,
    pub checkout: Option<CodexCheckoutState>,
    pub tmux_session: String,
    pub pane_id: String,
    pub state: CodexSessionState,
    pub is_compacting: bool,
    pub session_start_source: Option<String>,
    pub received_at_unix_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexCheckoutState {
    pub repository_root: Option<String>,
    pub repository_url: Option<String>,
    pub provider_host: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub checked_at_unix_ms: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryCheckoutState {
    pub world_id: Uuid,
    pub world_name: String,
    pub session_id: Uuid,
    pub cwd: String,
    pub checkout: CodexCheckoutState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryGitState {
    pub repository_id: u64,
    pub provider_host: String,
    pub repository: String,
    pub checkouts: Vec<RepositoryCheckoutState>,
    pub git_activity: Vec<GitActivity>,
    pub wt_tools_activity: Vec<WtToolsActivity>,
}

pub struct CodexSessionReportInput<'a> {
    pub world_id: Uuid,
    pub session_id: Uuid,
    pub cwd: &'a str,
    pub tmux_session: &'a str,
    pub pane_id: &'a str,
    pub state: Option<CodexSessionState>,
    pub is_compacting: Option<bool>,
    pub pane_generation: u64,
    pub pane_sequence: u64,
    pub session_start_source: Option<&'a str>,
}

pub struct CodexSessionGitContextInput<'a> {
    pub world_id: Uuid,
    pub session_id: Uuid,
    pub cwd: &'a str,
    pub tmux_session: &'a str,
    pub pane_id: &'a str,
    pub pane_generation: u64,
    pub repository_root: Option<&'a str>,
    pub repository_url: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub repository_target: Option<RepositoryTargetInput<'a>>,
    pub error: Option<&'a str>,
}

#[derive(Insertable)]
#[diesel(table_name = codex_session_reports)]
struct NewCodexSessionReport<'a> {
    world_id: String,
    session_id: String,
    cwd: &'a str,
    tmux_session: &'a str,
    pane_id: &'a str,
    state: &'static str,
    is_compacting: bool,
    pane_generation: i64,
    pane_sequence: i64,
    session_start_source: Option<&'a str>,
    received_at_unix_ms: i64,
}

#[derive(Insertable)]
#[diesel(table_name = codex_checkout_state)]
struct NewCodexCheckoutState<'a> {
    world_id: String,
    session_id: String,
    cwd: &'a str,
    repository_root: Option<&'a str>,
    repository_url: Option<&'a str>,
    repository_id: Option<i32>,
    branch: Option<&'a str>,
    checked_at_unix_ms: i64,
    error: Option<&'a str>,
    pane_id: &'a str,
    pane_generation: i64,
}

#[derive(Queryable)]
struct CodexSessionReportRow {
    world_id: String,
    world_name: String,
    session_id: String,
    cwd: String,
    tmux_session: String,
    pane_id: String,
    pane_generation: i64,
    state: String,
    is_compacting: bool,
    session_start_source: Option<String>,
    received_at_unix_ms: i64,
}

#[derive(Queryable)]
struct CodexCheckoutStateRow {
    repository_root: Option<String>,
    repository_url: Option<String>,
    provider_host: Option<String>,
    repository: Option<String>,
    branch: Option<String>,
    checked_at_unix_ms: i64,
    error: Option<String>,
}

impl Registry {
    pub fn upsert_codex_session_report(
        &self,
        input: CodexSessionReportInput<'_>,
    ) -> Result<bool, RegistryError> {
        if input.pane_generation == 0 || input.pane_sequence == 0 {
            return Err(RegistryError::InvalidData(
                "invalid Codex pane event order".into(),
            ));
        }
        validate_lifecycle_report(
            input.cwd,
            input.tmux_session,
            input.pane_id,
            input.session_start_source,
        )?;
        let received_at_unix_ms = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?
                .as_millis(),
        )
        .map_err(|_| RegistryError::InvalidData("system time is too large".into()))?;
        let pane_generation = i64::try_from(input.pane_generation)
            .map_err(|_| RegistryError::InvalidData("Codex pane generation is too large".into()))?;
        let pane_sequence = i64::try_from(input.pane_sequence)
            .map_err(|_| RegistryError::InvalidData("Codex pane sequence is too large".into()))?;
        self.immediate_transaction(|connection| {
            let latest = codex_session_reports::table
                .filter(codex_session_reports::world_id.eq(input.world_id.to_string()))
                .filter(codex_session_reports::tmux_session.eq(input.tmux_session))
                .filter(codex_session_reports::pane_id.eq(input.pane_id))
                .select((
                    codex_session_reports::pane_generation,
                    codex_session_reports::pane_sequence,
                ))
                .order_by(codex_session_reports::pane_generation.desc())
                .then_order_by(codex_session_reports::pane_sequence.desc())
                .first::<(i64, i64)>(connection)
                .optional()?;
            if latest.is_some_and(|latest| (pane_generation, pane_sequence) <= latest) {
                return Ok(false);
            }
            let existing = codex_session_reports::table
                .find((input.world_id.to_string(), input.session_id.to_string()))
                .select((
                    codex_session_reports::state,
                    codex_session_reports::is_compacting,
                ))
                .first::<(String, bool)>(connection)
                .optional()?;
            let state = input
                .state
                .or(existing
                    .as_ref()
                    .map(|(state, _)| CodexSessionState::parse(state))
                    .transpose()?)
                .unwrap_or(CodexSessionState::Unknown);
            let is_compacting = input
                .is_compacting
                .or(existing.map(|(_, is_compacting)| is_compacting))
                .unwrap_or(false);
            let report = NewCodexSessionReport {
                world_id: input.world_id.to_string(),
                session_id: input.session_id.to_string(),
                cwd: input.cwd,
                tmux_session: input.tmux_session,
                pane_id: input.pane_id,
                state: state.as_str(),
                is_compacting,
                pane_generation,
                pane_sequence,
                session_start_source: input.session_start_source,
                received_at_unix_ms,
            };
            if input
                .state
                .is_some_and(|state| state != CodexSessionState::Inactive)
            {
                diesel::update(
                    codex_session_reports::table
                        .filter(codex_session_reports::world_id.eq(&report.world_id))
                        .filter(codex_session_reports::session_id.ne(&report.session_id))
                        .filter(codex_session_reports::tmux_session.eq(report.tmux_session))
                        .filter(codex_session_reports::pane_id.eq(report.pane_id))
                        .filter(codex_session_reports::state.ne("inactive")),
                )
                .set((
                    codex_session_reports::state.eq("inactive"),
                    codex_session_reports::is_compacting.eq(false),
                    codex_session_reports::pane_generation.eq(pane_generation),
                    codex_session_reports::pane_sequence.eq(pane_sequence),
                    codex_session_reports::received_at_unix_ms.eq(received_at_unix_ms),
                ))
                .execute(connection)?;
            }
            let target = diesel::insert_into(codex_session_reports::table)
                .values(&report)
                .on_conflict((
                    codex_session_reports::world_id,
                    codex_session_reports::session_id,
                ))
                .do_update();
            target
                .set((
                    codex_session_reports::cwd.eq(report.cwd),
                    codex_session_reports::tmux_session.eq(report.tmux_session),
                    codex_session_reports::pane_id.eq(report.pane_id),
                    codex_session_reports::state.eq(report.state),
                    codex_session_reports::is_compacting.eq(report.is_compacting),
                    codex_session_reports::pane_generation.eq(report.pane_generation),
                    codex_session_reports::pane_sequence.eq(report.pane_sequence),
                    codex_session_reports::session_start_source.eq(report.session_start_source),
                    codex_session_reports::received_at_unix_ms.eq(report.received_at_unix_ms),
                ))
                .execute(connection)?;
            Ok(true)
        })
    }

    pub fn update_codex_session_git_context(
        &self,
        input: CodexSessionGitContextInput<'_>,
    ) -> Result<bool, RegistryError> {
        validate_lifecycle_report(input.cwd, input.tmux_session, input.pane_id, None)?;
        validate_checkout_context(
            input.repository_root,
            input.repository_url,
            input.git_branch,
        )?;
        if input
            .error
            .is_some_and(|value| value.is_empty() || value.len() > 1024)
        {
            return Err(RegistryError::InvalidData(
                "invalid Codex session Git error".into(),
            ));
        }
        let pane_generation = i64::try_from(input.pane_generation)
            .map_err(|_| RegistryError::InvalidData("Codex pane generation is too large".into()))?;
        let checked_at_unix_ms = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?
                .as_millis(),
        )
        .map_err(|_| RegistryError::InvalidData("system time is too large".into()))?;
        self.immediate_transaction(|connection| {
            let active = codex_session_reports::table
                .filter(codex_session_reports::world_id.eq(input.world_id.to_string()))
                .filter(codex_session_reports::session_id.eq(input.session_id.to_string()))
                .filter(codex_session_reports::cwd.eq(input.cwd))
                .filter(codex_session_reports::tmux_session.eq(input.tmux_session))
                .filter(codex_session_reports::pane_id.eq(input.pane_id))
                .filter(codex_session_reports::pane_generation.eq(pane_generation))
                .filter(codex_session_reports::state.ne("inactive"))
                .select(codex_session_reports::world_id)
                .first::<String>(connection)
                .optional()?;
            if active.is_none() {
                return Ok(false);
            }
            let repository_id = input
                .repository_target
                .map(|target| intern_repository(connection, target))
                .transpose()?;
            let checkout = NewCodexCheckoutState {
                world_id: input.world_id.to_string(),
                session_id: input.session_id.to_string(),
                cwd: input.cwd,
                repository_root: input.repository_root,
                repository_url: input.repository_url,
                repository_id,
                branch: input.git_branch,
                checked_at_unix_ms,
                error: input.error,
                pane_id: input.pane_id,
                pane_generation,
            };
            let target = diesel::insert_into(codex_checkout_state::table)
                .values(&checkout)
                .on_conflict((
                    codex_checkout_state::world_id,
                    codex_checkout_state::session_id,
                    codex_checkout_state::cwd,
                ))
                .do_update();
            if let Some(error) = input.error {
                target
                    .set((
                        codex_checkout_state::checked_at_unix_ms.eq(checked_at_unix_ms),
                        codex_checkout_state::error.eq(error),
                        codex_checkout_state::pane_id.eq(input.pane_id),
                        codex_checkout_state::pane_generation.eq(pane_generation),
                    ))
                    .execute(connection)?;
            } else {
                target
                    .set((
                        codex_checkout_state::repository_root.eq(input.repository_root),
                        codex_checkout_state::repository_url.eq(input.repository_url),
                        codex_checkout_state::repository_id.eq(repository_id),
                        codex_checkout_state::branch.eq(input.git_branch),
                        codex_checkout_state::checked_at_unix_ms.eq(checked_at_unix_ms),
                        codex_checkout_state::error.eq::<Option<&str>>(None),
                        codex_checkout_state::pane_id.eq(input.pane_id),
                        codex_checkout_state::pane_generation.eq(pane_generation),
                    ))
                    .execute(connection)?;
            }
            Ok(true)
        })
    }

    pub fn list_codex_session_reports(
        &self,
        owner: &str,
    ) -> Result<Vec<CodexSessionReport>, RegistryError> {
        self.read(|connection| {
            codex_session_reports::table
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .order(codex_session_reports::received_at_unix_ms.desc())
                .select((
                    codex_session_reports::world_id,
                    worlds::name,
                    codex_session_reports::session_id,
                    codex_session_reports::cwd,
                    codex_session_reports::tmux_session,
                    codex_session_reports::pane_id,
                    codex_session_reports::pane_generation,
                    codex_session_reports::state,
                    codex_session_reports::is_compacting,
                    codex_session_reports::session_start_source,
                    codex_session_reports::received_at_unix_ms,
                ))
                .load::<CodexSessionReportRow>(connection)?
                .into_iter()
                .map(|row| {
                    let checkout = load_current_checkout(connection, &row)?;
                    Ok(CodexSessionReport {
                        world_id: Uuid::parse_str(&row.world_id)
                            .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
                        world_name: row.world_name,
                        session_id: Uuid::parse_str(&row.session_id)
                            .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
                        cwd: row.cwd,
                        checkout,
                        tmux_session: row.tmux_session,
                        pane_id: row.pane_id,
                        state: CodexSessionState::parse(&row.state)?,
                        is_compacting: row.is_compacting,
                        session_start_source: row.session_start_source,
                        received_at_unix_ms: row.received_at_unix_ms,
                    })
                })
                .collect()
        })
    }

    pub fn repository_git_state(
        &self,
        owner: &str,
        provider_host: &str,
        repository: &str,
        git_before_id: Option<u64>,
        wt_tools_before_id: Option<u64>,
    ) -> Result<Option<RepositoryGitState>, RegistryError> {
        validate_target(provider_host, repository)?;
        let exists = self.read(|connection| {
            repositories::table
                .filter(repositories::provider_host.eq(provider_host))
                .filter(repositories::repository.eq(repository))
                .select(repositories::id)
                .first::<i32>(connection)
                .optional()
        })?;
        let checkouts: Vec<_> = self
            .list_codex_session_reports(owner)?
            .into_iter()
            .filter_map(|report| {
                let checkout = report.checkout?;
                (checkout.provider_host.as_deref() == Some(provider_host)
                    && checkout.repository.as_deref() == Some(repository))
                .then_some(RepositoryCheckoutState {
                    world_id: report.world_id,
                    world_name: report.world_name,
                    session_id: report.session_id,
                    cwd: report.cwd,
                    checkout,
                })
            })
            .collect();
        let git_activity = self.list_git_activity(
            owner,
            GitActivityQuery::Repository {
                provider_host: provider_host.to_owned(),
                repository: repository.to_owned(),
                before_id: git_before_id,
            },
        )?;
        let wt_tools_activity = self.list_wt_tools_activity(
            owner,
            WtToolsActivityQuery::Repository {
                provider_host: provider_host.to_owned(),
                repository: repository.to_owned(),
                before_id: wt_tools_before_id,
            },
        )?;
        if checkouts.is_empty() && git_activity.is_empty() && wt_tools_activity.is_empty() {
            return Ok(None);
        }
        Ok(Some(RepositoryGitState {
            repository_id: u64::try_from(exists.expect("repository state has a catalog row"))
                .map_err(|_| RegistryError::InvalidData("invalid repository ID".into()))?,
            provider_host: provider_host.to_owned(),
            repository: repository.to_owned(),
            checkouts,
            git_activity,
            wt_tools_activity,
        }))
    }
}

fn load_current_checkout(
    connection: &mut diesel::sqlite::SqliteConnection,
    report: &CodexSessionReportRow,
) -> Result<Option<CodexCheckoutState>, RegistryError> {
    if report.state == "inactive" {
        return Ok(None);
    }
    codex_checkout_state::table
        .left_join(repositories::table)
        .filter(codex_checkout_state::world_id.eq(&report.world_id))
        .filter(codex_checkout_state::session_id.eq(&report.session_id))
        .filter(codex_checkout_state::cwd.eq(&report.cwd))
        .filter(codex_checkout_state::pane_id.eq(&report.pane_id))
        .filter(codex_checkout_state::pane_generation.eq(report.pane_generation))
        .select((
            codex_checkout_state::repository_root,
            codex_checkout_state::repository_url,
            repositories::provider_host.nullable(),
            repositories::repository.nullable(),
            codex_checkout_state::branch,
            codex_checkout_state::checked_at_unix_ms,
            codex_checkout_state::error,
        ))
        .first::<CodexCheckoutStateRow>(connection)
        .optional()?
        .map(|row| {
            Ok(CodexCheckoutState {
                repository_root: row.repository_root,
                repository_url: row.repository_url,
                provider_host: row.provider_host,
                repository: row.repository,
                branch: row.branch,
                checked_at_unix_ms: row.checked_at_unix_ms,
                error: row.error,
            })
        })
        .transpose()
}

fn validate_lifecycle_report(
    cwd: &str,
    tmux_session: &str,
    pane_id: &str,
    session_start_source: Option<&str>,
) -> Result<(), RegistryError> {
    if !cwd.starts_with('/') || cwd.len() > 4096 {
        return Err(RegistryError::InvalidData(
            "invalid Codex session working directory".into(),
        ));
    }
    if tmux_session != "wt-host" {
        return Err(RegistryError::InvalidData(
            "invalid Codex session tmux session".into(),
        ));
    }
    if !pane_id.strip_prefix('%').is_some_and(|number| {
        !number.is_empty() && number.len() <= 16 && number.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return Err(RegistryError::InvalidData(
            "invalid Codex session pane ID".into(),
        ));
    }
    if session_start_source.is_some_and(|source| {
        source.is_empty()
            || source.len() > 64
            || !source
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    }) {
        return Err(RegistryError::InvalidData(
            "invalid Codex session start source".into(),
        ));
    }
    Ok(())
}

fn validate_checkout_context(
    repository_root: Option<&str>,
    repository_url: Option<&str>,
    git_branch: Option<&str>,
) -> Result<(), RegistryError> {
    if repository_root.is_some_and(|value| !value.starts_with('/') || value.len() > 4096)
        || repository_url.is_some_and(|value| value.is_empty() || value.len() > 4096)
        || git_branch.is_some_and(|value| value.is_empty() || value.len() > 1024)
    {
        return Err(RegistryError::InvalidData(
            "invalid Codex session Git context".into(),
        ));
    }
    Ok(())
}
