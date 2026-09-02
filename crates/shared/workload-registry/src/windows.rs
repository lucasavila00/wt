use crate::schema::{window_input, window_output, windows};
use crate::{Store, StoreError};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use wt_world::{WindowId, WorldId};

pub const WINDOW_OUTPUT_RETENTION_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    Starting,
    Running,
    Exited,
    Stopped,
}

impl WindowState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Stopped => "stopped",
        }
    }
}

impl FromStr for WindowState {
    type Err = StoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "exited" => Ok(Self::Exited),
            "stopped" => Ok(Self::Stopped),
            _ => Err(StoreError::InvalidData(format!(
                "unknown window state {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewWindow {
    pub window_id: WindowId,
    pub world_id: WorldId,
    pub owner: String,
    pub tmux_window_id: Option<String>,
    pub control_token: String,
    pub control_token_hash: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWindow {
    pub window_id: WindowId,
    pub world_id: WorldId,
    pub owner: String,
    pub tmux_window_id: Option<String>,
    pub control_token: String,
    pub control_token_hash: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub state: WindowState,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    pub next_output_record_id: u64,
    pub oldest_available: u64,
    pub retained_output_bytes: u64,
    pub next_input_sequence_id: u64,
    pub output_offset: u64,
    pub screen: Option<String>,
    pub screen_observed_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowOutput {
    pub record_id: u64,
    pub channel: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowOutputPage {
    pub output: Vec<WindowOutput>,
    pub next_after: u64,
    pub oldest_available: u64,
    pub gap: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowInput {
    pub sequence_id: u64,
    pub request_id: uuid::Uuid,
    pub data: Vec<u8>,
}

#[derive(Insertable)]
#[diesel(table_name = windows)]
struct NewWindowRow<'a> {
    window_id: String,
    world_id: String,
    owner: &'a str,
    tmux_window_id: Option<&'a str>,
    control_token: &'a str,
    control_token_hash: &'a str,
    argv_json: String,
    cwd: &'a str,
    state: &'static str,
    created_at_unix_ms: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = windows)]
struct WindowRow {
    window_id: String,
    world_id: String,
    owner: String,
    tmux_window_id: Option<String>,
    control_token: String,
    control_token_hash: String,
    argv_json: String,
    cwd: String,
    state: String,
    exit_code: Option<i32>,
    exit_signal: Option<i32>,
    next_output_record_id: i64,
    oldest_available: i64,
    retained_output_bytes: i64,
    next_input_sequence_id: i64,
    output_offset: i64,
    screen: Option<String>,
    screen_observed_at_unix_ms: Option<i64>,
    created_at_unix_ms: i64,
}

#[derive(Queryable)]
struct OutputRow {
    record_id: i64,
    channel: String,
    data: Vec<u8>,
}

impl Store {
    pub fn insert_window(&self, window: &NewWindow) -> Result<(), StoreError> {
        let argv_json = serde_json::to_string(&window.argv)
            .map_err(|error| StoreError::InvalidData(format!("encode window argv: {error}")))?;
        self.registry.transaction(|connection| {
            diesel::insert_into(windows::table)
                .values(NewWindowRow {
                    window_id: window.window_id.to_string(),
                    world_id: window.world_id.to_string(),
                    owner: &window.owner,
                    tmux_window_id: window.tmux_window_id.as_deref(),
                    control_token: &window.control_token,
                    control_token_hash: &window.control_token_hash,
                    argv_json,
                    cwd: &window.cwd,
                    state: if window.tmux_window_id.is_some() {
                        WindowState::Running.as_str()
                    } else {
                        WindowState::Starting.as_str()
                    },
                    created_at_unix_ms: now_unix_ms(),
                })
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn get_owned_window(
        &self,
        owner: &str,
        window_id: WindowId,
    ) -> Result<StoredWindow, StoreError> {
        self.registry.read(|connection| {
            windows::table
                .find(window_id.to_string())
                .filter(windows::owner.eq(owner))
                .select(WindowRow::as_select())
                .first(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?
                .try_into()
        })
    }

    /// Resolves a guest-native tmux window after the caller authenticated its world.
    pub fn window_id_by_tmux(
        &self,
        world_id: WorldId,
        tmux_window_id: &str,
    ) -> Result<WindowId, StoreError> {
        let value = self.registry.read(|connection| {
            windows::table
                .filter(windows::world_id.eq(world_id.to_string()))
                .filter(windows::tmux_window_id.eq(tmux_window_id))
                .select(windows::window_id)
                .first::<String>(connection)
                .optional()
        })?;
        value
            .ok_or(StoreError::NotFound)?
            .parse()
            .map_err(|error| StoreError::InvalidData(format!("invalid window ID: {error}")))
    }

    pub fn activate_window(
        &self,
        window_id: WindowId,
        tmux_window_id: &str,
    ) -> Result<(), StoreError> {
        let changed = self.registry.transaction(|connection| {
            diesel::update(windows::table.find(window_id.to_string()))
                .set((
                    windows::tmux_window_id.eq(tmux_window_id),
                    windows::state.eq(WindowState::Running.as_str()),
                ))
                .execute(connection)
        })?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn update_window_observation(
        &self,
        window_id: WindowId,
        state: WindowState,
        exit_code: Option<i32>,
        exit_signal: Option<i32>,
        screen: Option<&str>,
        screen_observed_at_unix_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            let changed = diesel::update(windows::table.find(window_id.to_string()))
                .set((
                    windows::state.eq(state.as_str()),
                    windows::exit_code.eq(exit_code),
                    windows::exit_signal.eq(exit_signal),
                    windows::screen.eq(screen),
                    windows::screen_observed_at_unix_ms.eq(screen_observed_at_unix_ms),
                ))
                .execute(connection)?;
            if changed == 0 {
                return Err(StoreError::NotFound);
            }
            Ok(())
        })
    }

    /// Commits one guest observation and its ordered output as one cursor advance.
    #[allow(clippy::too_many_arguments)]
    pub fn record_window_observation(
        &self,
        window_id: WindowId,
        previous_output_offset: u64,
        output_offset: u64,
        records: &[(String, Vec<u8>)],
        state: WindowState,
        exit_code: Option<i32>,
        exit_signal: Option<i32>,
        screen: &str,
        screen_observed_at_unix_ms: i64,
    ) -> Result<(), StoreError> {
        if output_offset < previous_output_offset {
            return Err(StoreError::InvalidData(
                "window output cursor moved backwards".into(),
            ));
        }
        self.registry.immediate_transaction(|connection| {
            let key = window_id.to_string();
            let (stored_offset, mut next, mut retained) = windows::table
                .find(&key)
                .select((
                    windows::output_offset,
                    windows::next_output_record_id,
                    windows::retained_output_bytes,
                ))
                .first::<(i64, i64, i64)>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?;
            if stored_offset != previous_output_offset as i64 {
                return Err(StoreError::Conflict);
            }
            for (channel, data) in records {
                if !matches!(channel.as_str(), "stdout" | "stderr") {
                    return Err(StoreError::InvalidData(format!(
                        "invalid output channel {channel:?}"
                    )));
                }
                diesel::insert_into(window_output::table)
                    .values((
                        window_output::window_id.eq(&key),
                        window_output::record_id.eq(next),
                        window_output::channel.eq(channel),
                        window_output::data.eq(data),
                    ))
                    .execute(connection)?;
                next += 1;
                retained = retained.saturating_add(data.len() as i64);
            }
            while retained > WINDOW_OUTPUT_RETENTION_BYTES as i64 {
                let oldest = window_output::table
                    .filter(window_output::window_id.eq(&key))
                    .order(window_output::record_id)
                    .select((window_output::record_id, window_output::data))
                    .first::<(i64, Vec<u8>)>(connection)?;
                diesel::delete(window_output::table.find((&key, oldest.0))).execute(connection)?;
                retained -= oldest.1.len() as i64;
            }
            let oldest = window_output::table
                .filter(window_output::window_id.eq(&key))
                .select(diesel::dsl::min(window_output::record_id))
                .first::<Option<i64>>(connection)?
                .unwrap_or(next);
            diesel::update(windows::table.find(&key))
                .set((
                    windows::state.eq(state.as_str()),
                    windows::exit_code.eq(exit_code),
                    windows::exit_signal.eq(exit_signal),
                    windows::next_output_record_id.eq(next),
                    windows::oldest_available.eq(oldest),
                    windows::retained_output_bytes.eq(retained),
                    windows::output_offset.eq(output_offset as i64),
                    windows::screen.eq(screen),
                    windows::screen_observed_at_unix_ms.eq(screen_observed_at_unix_ms),
                ))
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn append_window_output(
        &self,
        window_id: WindowId,
        records: &[(String, Vec<u8>)],
    ) -> Result<(), StoreError> {
        self.registry.immediate_transaction(|connection| {
            let key = window_id.to_string();
            let (mut next, mut retained) = windows::table
                .find(&key)
                .select((
                    windows::next_output_record_id,
                    windows::retained_output_bytes,
                ))
                .first::<(i64, i64)>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?;
            for (channel, data) in records {
                if !matches!(channel.as_str(), "stdout" | "stderr") {
                    return Err(StoreError::InvalidData(format!(
                        "invalid output channel {channel:?}"
                    )));
                }
                diesel::insert_into(window_output::table)
                    .values((
                        window_output::window_id.eq(&key),
                        window_output::record_id.eq(next),
                        window_output::channel.eq(channel),
                        window_output::data.eq(data),
                    ))
                    .execute(connection)?;
                next += 1;
                retained = retained.saturating_add(i64::try_from(data.len()).unwrap_or(i64::MAX));
            }
            while retained > WINDOW_OUTPUT_RETENTION_BYTES as i64 {
                let oldest = window_output::table
                    .filter(window_output::window_id.eq(&key))
                    .order(window_output::record_id)
                    .select((window_output::record_id, window_output::data))
                    .first::<(i64, Vec<u8>)>(connection)?;
                diesel::delete(window_output::table.find((&key, oldest.0))).execute(connection)?;
                retained = retained.saturating_sub(oldest.1.len() as i64);
            }
            let oldest = window_output::table
                .filter(window_output::window_id.eq(&key))
                .select(diesel::dsl::min(window_output::record_id))
                .first::<Option<i64>>(connection)?
                .unwrap_or(next);
            diesel::update(windows::table.find(&key))
                .set((
                    windows::next_output_record_id.eq(next),
                    windows::oldest_available.eq(oldest),
                    windows::retained_output_bytes.eq(retained),
                ))
                .execute(connection)?;
            Ok(())
        })
    }

    pub fn window_output(
        &self,
        window_id: WindowId,
        after: u64,
        limit: u32,
    ) -> Result<WindowOutputPage, StoreError> {
        self.registry.transaction(|connection| {
            let window = windows::table
                .find(window_id.to_string())
                .select((windows::oldest_available, windows::next_output_record_id))
                .first::<(i64, i64)>(connection)
                .optional()?
                .ok_or(StoreError::NotFound)?;
            let oldest_available = u64::try_from(window.0)
                .map_err(|_| StoreError::InvalidData("negative oldest output cursor".into()))?;
            let effective_after = after.max(oldest_available.saturating_sub(1));
            let rows = window_output::table
                .filter(window_output::window_id.eq(window_id.to_string()))
                .filter(window_output::record_id.gt(effective_after as i64))
                .order(window_output::record_id)
                .limit(i64::from(limit))
                .select((
                    window_output::record_id,
                    window_output::channel,
                    window_output::data,
                ))
                .load::<OutputRow>(connection)?;
            let output = rows
                .into_iter()
                .map(|row| {
                    Ok(WindowOutput {
                        record_id: u64::try_from(row.record_id).map_err(|_| {
                            StoreError::InvalidData("negative output record ID".into())
                        })?,
                        channel: row.channel,
                        data: row.data,
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()?;
            let next_after = output
                .last()
                .map_or(effective_after, |record| record.record_id);
            Ok(WindowOutputPage {
                output,
                next_after,
                oldest_available,
                gap: after.saturating_add(1) < oldest_available,
            })
        })
    }

    pub fn enqueue_window_input(
        &self,
        window_id: WindowId,
        request_id: uuid::Uuid,
        data: &[u8],
    ) -> Result<u64, StoreError> {
        self.registry.immediate_transaction(|connection| {
            let key = window_id.to_string();
            let exists = windows::table
                .find(&key)
                .select(windows::window_id)
                .first::<String>(connection)
                .optional()?
                .is_some();
            if !exists {
                return Err(StoreError::NotFound);
            }
            if let Some((sequence_id, existing_data)) = window_input::table
                .filter(window_input::window_id.eq(&key))
                .filter(window_input::request_id.eq(request_id.to_string()))
                .select((window_input::sequence_id, window_input::data))
                .first::<(i64, Vec<u8>)>(connection)
                .optional()?
            {
                if existing_data != data {
                    return Err(StoreError::Conflict);
                }
                return u64::try_from(sequence_id)
                    .map_err(|_| StoreError::InvalidData("negative input sequence ID".into()));
            }
            let sequence_id = windows::table
                .find(&key)
                .select(windows::next_input_sequence_id)
                .first::<i64>(connection)?;
            diesel::insert_into(window_input::table)
                .values((
                    window_input::window_id.eq(&key),
                    window_input::sequence_id.eq(sequence_id),
                    window_input::request_id.eq(request_id.to_string()),
                    window_input::data.eq(data),
                ))
                .execute(connection)?;
            diesel::update(windows::table.find(&key))
                .set(windows::next_input_sequence_id.eq(sequence_id + 1))
                .execute(connection)?;
            u64::try_from(sequence_id)
                .map_err(|_| StoreError::InvalidData("negative input sequence ID".into()))
        })
    }

    pub fn pending_window_input(
        &self,
        window_id: WindowId,
    ) -> Result<Vec<WindowInput>, StoreError> {
        self.registry.read(|connection| {
            window_input::table
                .filter(window_input::window_id.eq(window_id.to_string()))
                .order(window_input::sequence_id)
                .select((
                    window_input::sequence_id,
                    window_input::request_id,
                    window_input::data,
                ))
                .load::<(i64, String, Vec<u8>)>(connection)?
                .into_iter()
                .map(|(sequence_id, request_id, data)| {
                    Ok(WindowInput {
                        sequence_id: u64::try_from(sequence_id).map_err(|_| {
                            StoreError::InvalidData("negative input sequence ID".into())
                        })?,
                        request_id: request_id.parse().map_err(|error| {
                            StoreError::InvalidData(format!("invalid input request ID: {error}"))
                        })?,
                        data,
                    })
                })
                .collect()
        })
    }

    pub fn acknowledge_window_input(
        &self,
        window_id: WindowId,
        through: u64,
    ) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            diesel::delete(
                window_input::table
                    .filter(window_input::window_id.eq(window_id.to_string()))
                    .filter(window_input::sequence_id.le(through as i64)),
            )
            .execute(connection)?;
            Ok(())
        })
    }

    pub fn delete_owned_window(&self, owner: &str, window_id: WindowId) -> Result<(), StoreError> {
        let changed = self.registry.transaction(|connection| {
            diesel::delete(
                windows::table
                    .find(window_id.to_string())
                    .filter(windows::owner.eq(owner)),
            )
            .execute(connection)
        })?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn windows_for_world(&self, world_id: WorldId) -> Result<Vec<StoredWindow>, StoreError> {
        self.registry.read(|connection| {
            windows::table
                .filter(windows::world_id.eq(world_id.to_string()))
                .order(windows::created_at_unix_ms)
                .select(WindowRow::as_select())
                .load::<WindowRow>(connection)?
                .into_iter()
                .map(TryInto::try_into)
                .collect()
        })
    }

    pub fn mark_world_windows_stopped(&self, world_id: WorldId) -> Result<(), StoreError> {
        self.registry.transaction(|connection| {
            diesel::update(windows::table.filter(windows::world_id.eq(world_id.to_string())))
                .set((
                    windows::state.eq(WindowState::Stopped.as_str()),
                    windows::exit_code.eq(None::<i32>),
                    windows::exit_signal.eq(None::<i32>),
                ))
                .execute(connection)?;
            Ok(())
        })
    }
}

impl TryFrom<WindowRow> for StoredWindow {
    type Error = StoreError;

    fn try_from(row: WindowRow) -> Result<Self, Self::Error> {
        let _ = row.created_at_unix_ms;
        Ok(Self {
            window_id: row
                .window_id
                .parse()
                .map_err(|error| StoreError::InvalidData(format!("invalid window ID: {error}")))?,
            world_id: row
                .world_id
                .parse()
                .map_err(|error| StoreError::InvalidData(format!("invalid world ID: {error}")))?,
            owner: row.owner,
            tmux_window_id: row.tmux_window_id,
            control_token: row.control_token,
            control_token_hash: row.control_token_hash,
            argv: serde_json::from_str(&row.argv_json).map_err(|error| {
                StoreError::InvalidData(format!("invalid window argv: {error}"))
            })?,
            cwd: row.cwd,
            state: row.state.parse()?,
            exit_code: row.exit_code,
            exit_signal: row.exit_signal,
            next_output_record_id: u64::try_from(row.next_output_record_id)
                .map_err(|_| StoreError::InvalidData("negative next output cursor".into()))?,
            oldest_available: u64::try_from(row.oldest_available)
                .map_err(|_| StoreError::InvalidData("negative oldest output cursor".into()))?,
            retained_output_bytes: u64::try_from(row.retained_output_bytes)
                .map_err(|_| StoreError::InvalidData("negative retained output bytes".into()))?,
            next_input_sequence_id: u64::try_from(row.next_input_sequence_id)
                .map_err(|_| StoreError::InvalidData("negative next input sequence ID".into()))?,
            output_offset: u64::try_from(row.output_offset)
                .map_err(|_| StoreError::InvalidData("negative output offset".into()))?,
            screen: row.screen,
            screen_observed_at_unix_ms: row.screen_observed_at_unix_ms,
        })
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
