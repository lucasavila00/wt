use crate::schema::{repositories, world_git_activity, world_wt_tools_activity, worlds};
use crate::{Registry, RegistryError};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const ACTIVITY_PAGE_SIZE: i64 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitActivityKind {
    Service,
    BranchUpdate,
}

impl GitActivityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::BranchUpdate => "branch_update",
        }
    }

    fn parse(value: &str) -> Result<Self, RegistryError> {
        match value {
            "service" => Ok(Self::Service),
            "branch_update" => Ok(Self::BranchUpdate),
            _ => Err(RegistryError::InvalidData(format!(
                "invalid Git activity kind: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitActivityInput<'a> {
    pub world_id: Uuid,
    pub kind: GitActivityKind,
    pub provider_host: &'a str,
    pub repository: &'a str,
    pub git_service: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub previous_oid: Option<&'a str>,
    pub new_oid: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepositoryTargetInput<'a> {
    pub provider_host: &'a str,
    pub repository: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitActivity {
    pub id: u64,
    pub world_id: Uuid,
    pub world_name: String,
    pub recorded_at_unix_ms: u64,
    pub kind: GitActivityKind,
    pub provider_host: String,
    pub repository: String,
    pub git_service: Option<String>,
    pub branch: Option<String>,
    pub previous_oid: Option<String>,
    pub new_oid: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitActivityQuery {
    World {
        world_id: Uuid,
        before_id: Option<u64>,
    },
    Repository {
        provider_host: String,
        repository: String,
        before_id: Option<u64>,
    },
    Branch {
        provider_host: String,
        repository: String,
        branch: String,
        before_id: Option<u64>,
    },
}

#[derive(Clone, Debug)]
pub struct WtToolsActivityInput<'a> {
    pub world_id: Uuid,
    pub provider_host: &'a str,
    pub repository: &'a str,
    pub action: &'a str,
    pub branch: Option<&'a str>,
    pub change_request: Option<&'a str>,
    pub request_json: &'a str,
    pub response_json: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WtToolsActivity {
    pub id: u64,
    pub world_id: Uuid,
    pub world_name: String,
    pub recorded_at_unix_ms: u64,
    pub provider_host: String,
    pub repository: String,
    pub action: String,
    pub branch: Option<String>,
    pub change_request: Option<String>,
    pub request_json: String,
    pub response_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WtToolsActivityQuery {
    World {
        world_id: Uuid,
        before_id: Option<u64>,
    },
    Repository {
        provider_host: String,
        repository: String,
        before_id: Option<u64>,
    },
    Branch {
        provider_host: String,
        repository: String,
        branch: String,
        before_id: Option<u64>,
    },
    ChangeRequest {
        provider_host: String,
        repository: String,
        change_request: String,
        before_id: Option<u64>,
    },
}

#[derive(Insertable)]
#[diesel(table_name = world_git_activity)]
struct NewGitActivity<'a> {
    world_id: String,
    recorded_at_unix_ms: i64,
    kind: &'a str,
    repository_id: i32,
    git_service: Option<&'a str>,
    branch: Option<&'a str>,
    previous_oid: Option<&'a str>,
    new_oid: Option<&'a str>,
}

#[derive(Insertable)]
#[diesel(table_name = world_wt_tools_activity)]
struct NewWtToolsActivity<'a> {
    world_id: String,
    recorded_at_unix_ms: i64,
    repository_id: i32,
    action: &'a str,
    branch: Option<&'a str>,
    change_request: Option<&'a str>,
    request_json: &'a str,
    response_json: &'a str,
}

#[derive(Queryable)]
struct GitActivityRow {
    id: i32,
    world_id: String,
    world_name: String,
    recorded_at_unix_ms: i64,
    kind: String,
    provider_host: String,
    repository: String,
    git_service: Option<String>,
    branch: Option<String>,
    previous_oid: Option<String>,
    new_oid: Option<String>,
}

#[derive(Queryable)]
struct WtToolsActivityRow {
    id: i32,
    world_id: String,
    world_name: String,
    recorded_at_unix_ms: i64,
    provider_host: String,
    repository: String,
    action: String,
    branch: Option<String>,
    change_request: Option<String>,
    request_json: String,
    response_json: String,
}

impl Registry {
    pub fn insert_git_activity(&self, input: GitActivityInput<'_>) -> Result<(), RegistryError> {
        validate_target(input.provider_host, input.repository)?;
        self.immediate_transaction(|connection| {
            let repository_id = intern_repository(
                connection,
                RepositoryTargetInput {
                    provider_host: input.provider_host,
                    repository: input.repository,
                },
            )?;
            diesel::insert_into(world_git_activity::table)
                .values(NewGitActivity {
                    world_id: input.world_id.to_string(),
                    recorded_at_unix_ms: now_unix_ms()?,
                    kind: input.kind.as_str(),
                    repository_id,
                    git_service: input.git_service,
                    branch: input.branch,
                    previous_oid: input.previous_oid,
                    new_oid: input.new_oid,
                })
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn insert_wt_tools_activity(
        &self,
        input: WtToolsActivityInput<'_>,
    ) -> Result<(), RegistryError> {
        validate_target(input.provider_host, input.repository)?;
        if input.action.is_empty() {
            return Err(RegistryError::InvalidData(
                "wt-tools action is empty".into(),
            ));
        }
        for (name, json) in [
            ("request", input.request_json),
            ("response", input.response_json),
        ] {
            serde_json::from_str::<serde_json::Value>(json).map_err(|error| {
                RegistryError::InvalidData(format!("invalid wt-tools {name} JSON: {error}"))
            })?;
        }
        self.immediate_transaction(|connection| {
            let repository_id = intern_repository(
                connection,
                RepositoryTargetInput {
                    provider_host: input.provider_host,
                    repository: input.repository,
                },
            )?;
            diesel::insert_into(world_wt_tools_activity::table)
                .values(NewWtToolsActivity {
                    world_id: input.world_id.to_string(),
                    recorded_at_unix_ms: now_unix_ms()?,
                    repository_id,
                    action: input.action,
                    branch: input.branch,
                    change_request: input.change_request,
                    request_json: input.request_json,
                    response_json: input.response_json,
                })
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn list_git_activity(
        &self,
        owner: &str,
        query: GitActivityQuery,
    ) -> Result<Vec<GitActivity>, RegistryError> {
        self.read(|connection| {
            let mut query_builder = world_git_activity::table
                .inner_join(repositories::table)
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .into_boxed();
            match query {
                GitActivityQuery::World {
                    world_id,
                    before_id,
                } => {
                    query_builder =
                        query_builder.filter(world_git_activity::world_id.eq(world_id.to_string()));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder
                            .filter(world_git_activity::id.lt(to_i32(before_id, "activity ID")?));
                    }
                }
                GitActivityQuery::Branch {
                    provider_host,
                    repository,
                    branch,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(repositories::provider_host.eq(provider_host))
                        .filter(repositories::repository.eq(repository))
                        .filter(world_git_activity::branch.eq(branch));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder
                            .filter(world_git_activity::id.lt(to_i32(before_id, "activity ID")?));
                    }
                }
                GitActivityQuery::Repository {
                    provider_host,
                    repository,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(repositories::provider_host.eq(provider_host))
                        .filter(repositories::repository.eq(repository));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder
                            .filter(world_git_activity::id.lt(to_i32(before_id, "activity ID")?));
                    }
                }
            }
            query_builder
                .order(world_git_activity::id.desc())
                .limit(ACTIVITY_PAGE_SIZE)
                .select((
                    world_git_activity::id,
                    world_git_activity::world_id,
                    worlds::name,
                    world_git_activity::recorded_at_unix_ms,
                    world_git_activity::kind,
                    repositories::provider_host,
                    repositories::repository,
                    world_git_activity::git_service,
                    world_git_activity::branch,
                    world_git_activity::previous_oid,
                    world_git_activity::new_oid,
                ))
                .load::<GitActivityRow>(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }

    pub fn list_wt_tools_activity(
        &self,
        owner: &str,
        query: WtToolsActivityQuery,
    ) -> Result<Vec<WtToolsActivity>, RegistryError> {
        self.read(|connection| {
            let mut query_builder = world_wt_tools_activity::table
                .inner_join(repositories::table)
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .into_boxed();
            match query {
                WtToolsActivityQuery::World {
                    world_id,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(world_wt_tools_activity::world_id.eq(world_id.to_string()));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder.filter(
                            world_wt_tools_activity::id.lt(to_i32(before_id, "activity ID")?),
                        );
                    }
                }
                WtToolsActivityQuery::Branch {
                    provider_host,
                    repository,
                    branch,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(repositories::provider_host.eq(provider_host))
                        .filter(repositories::repository.eq(repository))
                        .filter(world_wt_tools_activity::branch.eq(branch));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder.filter(
                            world_wt_tools_activity::id.lt(to_i32(before_id, "activity ID")?),
                        );
                    }
                }
                WtToolsActivityQuery::Repository {
                    provider_host,
                    repository,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(repositories::provider_host.eq(provider_host))
                        .filter(repositories::repository.eq(repository));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder.filter(
                            world_wt_tools_activity::id.lt(to_i32(before_id, "activity ID")?),
                        );
                    }
                }
                WtToolsActivityQuery::ChangeRequest {
                    provider_host,
                    repository,
                    change_request,
                    before_id,
                } => {
                    query_builder = query_builder
                        .filter(repositories::provider_host.eq(provider_host))
                        .filter(repositories::repository.eq(repository))
                        .filter(world_wt_tools_activity::change_request.eq(change_request));
                    if let Some(before_id) = before_id {
                        query_builder = query_builder.filter(
                            world_wt_tools_activity::id.lt(to_i32(before_id, "activity ID")?),
                        );
                    }
                }
            }
            query_builder
                .order(world_wt_tools_activity::id.desc())
                .limit(ACTIVITY_PAGE_SIZE)
                .select((
                    world_wt_tools_activity::id,
                    world_wt_tools_activity::world_id,
                    worlds::name,
                    world_wt_tools_activity::recorded_at_unix_ms,
                    repositories::provider_host,
                    repositories::repository,
                    world_wt_tools_activity::action,
                    world_wt_tools_activity::branch,
                    world_wt_tools_activity::change_request,
                    world_wt_tools_activity::request_json,
                    world_wt_tools_activity::response_json,
                ))
                .load::<WtToolsActivityRow>(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }
}

impl TryFrom<GitActivityRow> for GitActivity {
    type Error = RegistryError;

    fn try_from(row: GitActivityRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: u64::try_from(row.id)
                .map_err(|_| RegistryError::InvalidData("invalid activity ID".into()))?,
            world_id: Uuid::parse_str(&row.world_id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            world_name: row.world_name,
            recorded_at_unix_ms: u64::try_from(row.recorded_at_unix_ms)
                .map_err(|_| RegistryError::InvalidData("invalid activity time".into()))?,
            kind: GitActivityKind::parse(&row.kind)?,
            provider_host: row.provider_host,
            repository: row.repository,
            git_service: row.git_service,
            branch: row.branch,
            previous_oid: row.previous_oid,
            new_oid: row.new_oid,
        })
    }
}

impl TryFrom<WtToolsActivityRow> for WtToolsActivity {
    type Error = RegistryError;

    fn try_from(row: WtToolsActivityRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: u64::try_from(row.id)
                .map_err(|_| RegistryError::InvalidData("invalid activity ID".into()))?,
            world_id: Uuid::parse_str(&row.world_id)
                .map_err(|error| RegistryError::InvalidData(error.to_string()))?,
            world_name: row.world_name,
            recorded_at_unix_ms: u64::try_from(row.recorded_at_unix_ms)
                .map_err(|_| RegistryError::InvalidData("invalid activity time".into()))?,
            provider_host: row.provider_host,
            repository: row.repository,
            action: row.action,
            branch: row.branch,
            change_request: row.change_request,
            request_json: row.request_json,
            response_json: row.response_json,
        })
    }
}

pub(crate) fn intern_repository(
    connection: &mut SqliteConnection,
    target: RepositoryTargetInput<'_>,
) -> Result<i32, RegistryError> {
    validate_target(target.provider_host, target.repository)?;
    diesel::insert_into(repositories::table)
        .values((
            repositories::provider_host.eq(target.provider_host),
            repositories::repository.eq(target.repository),
        ))
        .on_conflict((repositories::provider_host, repositories::repository))
        .do_nothing()
        .execute(connection)?;
    repositories::table
        .filter(repositories::provider_host.eq(target.provider_host))
        .filter(repositories::repository.eq(target.repository))
        .select(repositories::id)
        .first(connection)
        .map_err(Into::into)
}

pub(crate) fn validate_target(provider_host: &str, repository: &str) -> Result<(), RegistryError> {
    if provider_host.is_empty() || repository.is_empty() {
        return Err(RegistryError::InvalidData(
            "activity target is empty".into(),
        ));
    }
    Ok(())
}

fn now_unix_ms() -> Result<i64, RegistryError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RegistryError::InvalidData(format!("system clock is before Unix epoch: {error}"))
        })?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| RegistryError::InvalidData("activity time is too large".into()))
}

fn to_i32(value: u64, field: &'static str) -> Result<i32, RegistryError> {
    i32::try_from(value).map_err(|_| RegistryError::InvalidData(format!("invalid {field}")))
}
