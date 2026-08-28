use super::control::{PaneCard, PaneCardIdentity, PaneCardKind};
pub(super) use super::model::ShellWorld;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use uuid::Uuid;
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextWorld;
use wt_control_protocol::{PaneObservation, World};

#[derive(Debug)]
pub(super) enum PaneContextSnapshot {
    Panes {
        context: String,
        panes: Vec<PaneObservation>,
    },
    Failure {
        message: String,
    },
}

pub(super) struct PaneCards {
    pub(super) cards: Vec<PaneCard>,
    pub(super) failures: Vec<String>,
}

impl ShellWorld {
    pub(super) fn from_inventory(item: &ContextWorld) -> Self {
        let mut world = Self::from_world(&item.context, &item.world);
        world.resources =
            wt_client::inventory::format_resources(&item.world, item.disk_usage_bytes);
        world.detail = wt_client::inventory::format_detail(item);
        world
    }

    pub(super) fn from_world(context: &str, world: &World) -> Self {
        let qualified_name = format!("{context}.{}", world.name);
        let control_alias = format!("{qualified_name}-direct");
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                world_id: world.world_id,
            },
            name: qualified_name,
            world_name: world.name.clone(),
            control_alias,
            status: world.status,
            resources: wt_client::inventory::format_resources(world, None),
            detail: world.last_error.as_deref().unwrap_or("-").into(),
            git_activity: super::git_activity::RepositoryActivity::Loading,
        }
    }

    #[cfg(test)]
    pub(super) fn test(name: &str, index: u128) -> Self {
        let (context, world_name) = name.split_once('.').unwrap_or(("local", name));
        Self {
            identity: super::model::WorldIdentity {
                context: context.into(),
                world_id: Uuid::from_u128(index).into(),
            },
            name: name.into(),
            world_name: wt_control_protocol::WorldName::parse(world_name).unwrap(),
            control_alias: format!("{name}-direct"),
            status: wt_control_protocol::WorldStatus::Running,
            resources: "2 CPU · 4G · 1G/32G disk".into(),
            detail: "-".into(),
            git_activity: super::git_activity::RepositoryActivity::Loading,
        }
    }
}

pub(super) fn load_snapshots(
    config: &ClientConfig,
    cancelled: &AtomicBool,
) -> Vec<PaneContextSnapshot> {
    config
        .contexts
        .iter()
        .take_while(|_| !cancelled.load(Ordering::Relaxed))
        .map(|context| {
            match wt_client::transport::call_pane_observations_with_timeout_until(
                context,
                super::CONTEXT_REQUEST_TIMEOUT,
                cancelled,
            ) {
                Ok(panes) => PaneContextSnapshot::Panes {
                    context: context.name.clone(),
                    panes,
                },
                Err(error) => PaneContextSnapshot::Failure {
                    message: error
                        .to_string()
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                },
            }
        })
        .collect()
}

pub(super) fn cards(snapshots: Vec<PaneContextSnapshot>, worlds: &[ShellWorld]) -> PaneCards {
    let mut cards = Vec::new();
    let mut failures = Vec::new();
    for snapshot in snapshots {
        match snapshot {
            PaneContextSnapshot::Panes { context, panes } => {
                match validate_context(&context, panes, worlds) {
                    Ok(mut context_cards) => cards.append(&mut context_cards),
                    Err(_) => cards.push(PaneCard::context_error(&context)),
                }
            }
            PaneContextSnapshot::Failure { message } => failures.push(message),
        }
    }
    cards.sort_by_key(|card| (card.sort_rank(), card.created_at_unix_ms()));
    PaneCards { cards, failures }
}

fn validate_context(
    context: &str,
    panes: Vec<PaneObservation>,
    worlds: &[ShellWorld],
) -> Result<Vec<PaneCard>, String> {
    let mut cards = Vec::new();
    let mut identities = BTreeSet::new();
    for pane in panes {
        if pane.created_at_unix_ms < 0
            || pane.changed_at_unix_ms < 0
            || pane.observed_at_unix_ms < 0
            || pane.changed_at_unix_ms > pane.observed_at_unix_ms
        {
            return Err(invalid(
                context,
                "nonnegative creation and observation timestamps",
                &pane.pane_id,
            ));
        }
        if let Some(frame) = &pane.frame {
            frame
                .validate()
                .map_err(|error| invalid(context, error, &pane.pane_id))?;
        }
        if !valid_pane_id(&pane.pane_id) {
            return Err(invalid(
                context,
                "pane_id is % plus 1-16 ASCII digits",
                &pane.pane_id,
            ));
        }
        if pane.tmux_session != "wt-host" {
            return Err(invalid(
                context,
                "tmux_session is wt-host",
                &pane.tmux_session,
            ));
        }
        let matching_worlds = worlds
            .iter()
            .filter(|world| {
                world.identity.context == context && world.identity.world_id == pane.world_id
            })
            .collect::<Vec<_>>();
        let [world] = matching_worlds.as_slice() else {
            return Err(invalid(
                context,
                "exactly one world matches (context, world_id)",
                &pane.world_id.to_string(),
            ));
        };
        if world.world_name != pane.world_name {
            return Err(invalid(
                context,
                "world_name matches inventory world_id",
                pane.world_name.as_str(),
            ));
        }
        let identity = PaneCardIdentity::Observation {
            context: context.into(),
            world_id: pane.world_id,
            tmux_session: pane.tmux_session.clone(),
            pane_id: pane.pane_id.clone(),
        };
        if !identities.insert(identity.clone()) {
            return Err(invalid(context, "unique pane identity", &pane.pane_id));
        }
        cards.push(PaneCard {
            identity,
            context: context.into(),
            created_at_unix_ms: Some(pane.created_at_unix_ms),
            observed_at_unix_ms: Some(pane.observed_at_unix_ms),
            kind: PaneCardKind::Observation {
                world_name: world.world_name.to_string(),
                changed_at_unix_ms: pane.changed_at_unix_ms,
                frame: pane.frame,
            },
        });
    }
    Ok(cards)
}

fn valid_pane_id(value: &str) -> bool {
    value.strip_prefix('%').is_some_and(|number| {
        !number.is_empty() && number.len() <= 16 && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn invalid(context: &str, invariant: &str, value: &str) -> String {
    format!("context {context}: failed invariant {invariant}; value {value:?}")
}

#[cfg(test)]
#[path = "pane/tests.rs"]
mod tests;
