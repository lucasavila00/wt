use crate::schema::{codex_session_reports, worlds};
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
    pub tmux_session: String,
    pub pane_id: String,
    pub state: CodexSessionState,
    pub received_at: i64,
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
    received_at: i64,
}

#[derive(Queryable)]
struct CodexSessionReportRow {
    world_id: String,
    world_name: String,
    session_id: String,
    cwd: String,
    tmux_session: String,
    pane_id: String,
    state: String,
    received_at: i64,
}

impl Registry {
    pub fn upsert_codex_session_report(
        &self,
        world_id: Uuid,
        session_id: Uuid,
        cwd: &str,
        tmux_session: &str,
        pane_id: &str,
        state: CodexSessionState,
    ) -> Result<(), RegistryError> {
        validate_report(cwd, tmux_session, pane_id)?;
        let received_at = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?
                .as_secs(),
        )
        .map_err(|_| RegistryError::InvalidData("system time is too large".into()))?;
        let report = NewCodexSessionReport {
            world_id: world_id.to_string(),
            session_id: session_id.to_string(),
            cwd,
            tmux_session,
            pane_id,
            state: state.as_str(),
            received_at,
        };
        self.read(|connection| {
            diesel::insert_into(codex_session_reports::table)
                .values(&report)
                .on_conflict((
                    codex_session_reports::world_id,
                    codex_session_reports::session_id,
                ))
                .do_update()
                .set((
                    codex_session_reports::cwd.eq(report.cwd),
                    codex_session_reports::tmux_session.eq(report.tmux_session),
                    codex_session_reports::pane_id.eq(report.pane_id),
                    codex_session_reports::state.eq(report.state),
                    codex_session_reports::received_at.eq(report.received_at),
                ))
                .execute(connection)?;
            Ok(())
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
                .order(codex_session_reports::received_at.desc())
                .select((
                    codex_session_reports::world_id,
                    worlds::name,
                    codex_session_reports::session_id,
                    codex_session_reports::cwd,
                    codex_session_reports::tmux_session,
                    codex_session_reports::pane_id,
                    codex_session_reports::state,
                    codex_session_reports::received_at,
                ))
                .load::<CodexSessionReportRow>(connection)?
                .into_iter()
                .map(|row| {
                    Ok(CodexSessionReport {
                        world_id: Uuid::parse_str(&row.world_id)
                            .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
                        world_name: row.world_name,
                        session_id: Uuid::parse_str(&row.session_id)
                            .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
                        cwd: row.cwd,
                        tmux_session: row.tmux_session,
                        pane_id: row.pane_id,
                        state: CodexSessionState::parse(&row.state)?,
                        received_at: row.received_at,
                    })
                })
                .collect()
        })
    }
}

fn validate_report(cwd: &str, tmux_session: &str, pane_id: &str) -> Result<(), RegistryError> {
    if !cwd.starts_with('/') || cwd.len() > 4096 {
        return Err(RegistryError::InvalidData(
            "invalid Codex session working directory".into(),
        ));
    }
    if !matches!(tmux_session, "wt-app" | "wt-host") {
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
    Ok(())
}
