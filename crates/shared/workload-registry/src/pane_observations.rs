use crate::schema::{pane_observations, worlds};
use crate::{Registry, RegistryError};
use diesel::prelude::*;
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};
use wt_world::WorldId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneObservation {
    pub world_id: WorldId,
    pub world_name: String,
    pub tmux_session: String,
    pub pane_id: String,
    pub changed_at_unix_ms: i64,
    pub observed_at_unix_ms: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct PaneObservationInput<'a> {
    pub tmux_session: &'a str,
    pub pane_id: &'a str,
    pub screen_fingerprint: &'a str,
}

#[derive(Insertable)]
#[diesel(table_name = pane_observations)]
struct NewPaneObservation<'a> {
    world_id: String,
    tmux_session: &'a str,
    pane_id: &'a str,
    screen_fingerprint: &'a str,
    changed_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

#[derive(Queryable)]
struct StoredPaneObservation {
    screen_fingerprint: String,
    changed_at_unix_ms: i64,
}

#[derive(Queryable)]
struct PaneObservationRow {
    world_id: String,
    world_name: String,
    tmux_session: String,
    pane_id: String,
    changed_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl Registry {
    pub fn replace_pane_observations(
        &self,
        world_id: WorldId,
        inputs: &[PaneObservationInput<'_>],
    ) -> Result<(), RegistryError> {
        let mut targets = BTreeSet::new();
        for input in inputs {
            validate(input)?;
            if !targets.insert((input.tmux_session, input.pane_id)) {
                return Err(RegistryError::InvalidData(
                    "duplicate pane observation".into(),
                ));
            }
        }
        let observed_at_unix_ms = now()?;
        self.immediate_transaction(|connection| {
            let world = world_id.to_string();
            for input in inputs {
                let existing = pane_observations::table
                    .find((&world, input.tmux_session, input.pane_id))
                    .select((
                        pane_observations::screen_fingerprint,
                        pane_observations::changed_at_unix_ms,
                    ))
                    .first::<StoredPaneObservation>(connection)
                    .optional()?;
                let changed_at_unix_ms = match existing {
                    Some(existing) if existing.screen_fingerprint == input.screen_fingerprint => {
                        existing.changed_at_unix_ms
                    }
                    _ => observed_at_unix_ms,
                };
                let observation = NewPaneObservation {
                    world_id: world.clone(),
                    tmux_session: input.tmux_session,
                    pane_id: input.pane_id,
                    screen_fingerprint: input.screen_fingerprint,
                    changed_at_unix_ms,
                    observed_at_unix_ms,
                };
                diesel::insert_into(pane_observations::table)
                    .values(&observation)
                    .on_conflict((
                        pane_observations::world_id,
                        pane_observations::tmux_session,
                        pane_observations::pane_id,
                    ))
                    .do_update()
                    .set((
                        pane_observations::screen_fingerprint.eq(observation.screen_fingerprint),
                        pane_observations::changed_at_unix_ms.eq(observation.changed_at_unix_ms),
                        pane_observations::observed_at_unix_ms.eq(observation.observed_at_unix_ms),
                    ))
                    .execute(connection)?;
            }
            let current = pane_observations::table
                .filter(pane_observations::world_id.eq(&world))
                .select((pane_observations::tmux_session, pane_observations::pane_id))
                .load::<(String, String)>(connection)?;
            for (tmux_session, pane_id) in current {
                if !targets.contains(&(tmux_session.as_str(), pane_id.as_str())) {
                    diesel::delete(pane_observations::table.find((&world, tmux_session, pane_id)))
                        .execute(connection)?;
                }
            }
            Ok(())
        })
    }

    pub fn list_pane_observations(
        &self,
        owner: &str,
    ) -> Result<Vec<PaneObservation>, RegistryError> {
        self.read(|connection| {
            pane_observations::table
                .inner_join(worlds::table)
                .filter(worlds::owner.eq(owner))
                .order((
                    worlds::created_at_unix_ms,
                    pane_observations::tmux_session,
                    pane_observations::pane_id,
                ))
                .select((
                    pane_observations::world_id,
                    worlds::name,
                    pane_observations::tmux_session,
                    pane_observations::pane_id,
                    pane_observations::changed_at_unix_ms,
                    pane_observations::observed_at_unix_ms,
                ))
                .load::<PaneObservationRow>(connection)?
                .into_iter()
                .map(|row| {
                    Ok(PaneObservation {
                        world_id: row.world_id.parse().map_err(|error| {
                            RegistryError::InvalidData(format!("invalid pane world ID: {error}"))
                        })?,
                        world_name: row.world_name,
                        tmux_session: row.tmux_session,
                        pane_id: row.pane_id,
                        changed_at_unix_ms: row.changed_at_unix_ms,
                        observed_at_unix_ms: row.observed_at_unix_ms,
                    })
                })
                .collect()
        })
    }
}

fn validate(input: &PaneObservationInput<'_>) -> Result<(), RegistryError> {
    if input.tmux_session != "wt-host"
        || !input.pane_id.strip_prefix('%').is_some_and(|value| {
            !value.is_empty()
                && value.len() <= 16
                && value.bytes().all(|byte| byte.is_ascii_digit())
        })
        || input.screen_fingerprint.len() != 64
        || !input
            .screen_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RegistryError::InvalidData(
            "invalid pane observation".into(),
        ));
    }
    Ok(())
}

fn now() -> Result<i64, RegistryError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| RegistryError::InvalidData(error.to_string()))?
            .as_millis(),
    )
    .map_err(|_| RegistryError::InvalidData("system time is too large".into()))
}
