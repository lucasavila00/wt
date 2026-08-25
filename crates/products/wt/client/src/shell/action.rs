use super::action_queue::{ActionId, Intent, ShellActionQueue};
use super::model::{ShellModel, ShellWorld};
use super::refresh::WorldRefresh;
use super::session::{SessionSet, StartTask};
use super::{codex, delete, session_viewport, ControlFlows, ShellRuntime};
use anyhow::Result;
use ratatui::layout::Rect;

pub(super) enum Task {
    Create { id: ActionId, phase: CreatePhase },
    Delete { id: ActionId, task: delete::Task },
    Focus { id: ActionId },
    Reconnect { id: ActionId, task: StartTask },
}

pub(super) enum CreatePhase {
    Provision(Box<crate::create::Flow>),
    Connect {
        world: Box<ShellWorld>,
        task: StartTask,
    },
}

impl Task {
    pub(super) fn is_focus(&self, id: ActionId) -> bool {
        matches!(self, Self::Focus { id: active } if *active == id)
    }

    pub(super) fn blocks_input(&self) -> bool {
        matches!(
            self,
            Self::Create {
                phase: CreatePhase::Provision(flow),
                ..
            } if flow.blocks_input()
        )
    }

    pub(super) fn blocking_create_mut(&mut self) -> Option<(ActionId, &mut crate::create::Flow)> {
        match self {
            Self::Create {
                id,
                phase: CreatePhase::Provision(flow),
            } if flow.blocks_input() => Some((*id, flow)),
            _ => None,
        }
    }

    pub(super) fn render_overlay(&self, frame: &mut ratatui::Frame<'_>) {
        if let Self::Create {
            phase: CreatePhase::Provision(flow),
            ..
        } = self
        {
            if flow.blocks_input() {
                flow.render_overlay(frame, frame.area());
            }
        }
    }

    fn id(&self) -> ActionId {
        match self {
            Self::Create { id, .. }
            | Self::Delete { id, .. }
            | Self::Focus { id }
            | Self::Reconnect { id, .. } => *id,
        }
    }
}

pub(super) fn start_next(
    flows: &mut ControlFlows,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    runtime: &ShellRuntime<'_>,
    area: Rect,
) -> bool {
    if flows.task.is_some() {
        return false;
    }
    let Some(active) = flows.actions.activate_next("Starting").cloned() else {
        return false;
    };
    let id = active.entry.id;
    let started = match active.entry.intent {
        Intent::Create(input) => crate::create::Flow::start(runtime.config, input).map(|flow| {
            model.show_worlds();
            flows.actions.update_phase(id, "Creating world");
            Task::Create {
                id,
                phase: CreatePhase::Provision(Box::new(flow)),
            }
        }),
        Intent::Delete(world) => delete::Task::start(runtime.config, world).map(|task| {
            flows.actions.update_phase(id, "Deleting world");
            Task::Delete { id, task }
        }),
        Intent::OpenCodex(target) => {
            if runtime
                .focus
                .start_action(id, sessions, model, target.clone())
            {
                flows.actions.update_phase(id, "Focusing session");
                Ok(Task::Focus { id })
            } else {
                model.finish_codex_open(&target, None, true);
                Err(anyhow::anyhow!("Codex session is no longer openable"))
            }
        }
        Intent::Reconnect(identity) => {
            let Some(index) = model.world_index(&identity) else {
                flows.actions.acknowledge(id, false);
                flows.action_error = Some("world is no longer available to reconnect".into());
                return true;
            };
            let (rows, columns) = session_viewport(model, area);
            sessions.start_reconnect(index, rows, columns).map(|task| {
                flows.actions.update_phase(id, "Reconnecting world");
                Task::Reconnect { id, task }
            })
        }
    };
    match started {
        Ok(task) => flows.task = Some(task),
        Err(error) => {
            flows.actions.acknowledge(id, false);
            flows.action_error = Some(format!("{error:#}"));
        }
    }
    true
}

pub(super) fn poll(
    flows: &mut ControlFlows,
    sessions: &mut SessionSet,
    model: &mut ShellModel,
    refresh: &WorldRefresh,
    area: Rect,
) -> Result<bool> {
    let Some(mut task) = flows.task.take() else {
        return Ok(false);
    };
    if !flows.actions.is_active(task.id()) {
        return Ok(false);
    }
    let mut keep = true;
    let changed = match &mut task {
        Task::Create { id, phase } => match phase {
            CreatePhase::Provision(flow) => match flow.poll() {
                crate::create::FlowAction::None => false,
                crate::create::FlowAction::Changed => {
                    if let Some(status) = flow.status() {
                        flows.actions.update_phase(*id, status);
                    }
                    true
                }
                crate::create::FlowAction::Created(created) => {
                    let world = codex::ShellWorld::from_world(&created.context, &created.world);
                    refresh.invalidate();
                    let (rows, columns) = session_viewport(model, area);
                    match sessions.start_world(&world, rows, columns) {
                        Ok(session_task) => {
                            flows.actions.update_phase(*id, "Connecting world session");
                            *phase = CreatePhase::Connect {
                                world: Box::new(world),
                                task: session_task,
                            };
                        }
                        Err(error) => {
                            flows.actions.acknowledge(*id, false);
                            flows.action_error = Some(format!("{error:#}"));
                            keep = false;
                        }
                    }
                    true
                }
                crate::create::FlowAction::Failed(error) => {
                    flows.actions.acknowledge(*id, false);
                    flows.action_error = Some(error);
                    keep = false;
                    true
                }
                crate::create::FlowAction::Cancel => {
                    flows.actions.acknowledge(*id, false);
                    keep = false;
                    true
                }
                crate::create::FlowAction::Cancelling => true,
                crate::create::FlowAction::Submit(_) => {
                    unreachable!("active creation submitted")
                }
            },
            CreatePhase::Connect { world, task } => match task.poll() {
                None => false,
                Some(Ok(started)) => {
                    let succeeded = sessions.finish_add(started);
                    if succeeded {
                        let mut worlds = model.worlds().to_vec();
                        worlds.push((**world).clone());
                        model.reconcile_worlds(worlds);
                    }
                    flows.actions.acknowledge(*id, succeeded);
                    keep = false;
                    true
                }
                Some(Err(error)) => fail(flows, *id, error, &mut keep),
            },
        },
        Task::Delete { id, task } => match task.poll() {
            None => false,
            Some(Ok(identity)) => {
                refresh.invalidate();
                let worlds = model
                    .worlds()
                    .iter()
                    .filter(|world| world.identity != identity)
                    .cloned()
                    .collect::<Vec<_>>();
                let (rows, columns) = session_viewport(model, area);
                sessions.reconcile(&worlds, rows, columns)?;
                model.reconcile_worlds(worlds);
                flows.actions.acknowledge(*id, true);
                keep = false;
                true
            }
            Some(Err(error)) => fail(flows, *id, error, &mut keep),
        },
        Task::Focus { .. } => false,
        Task::Reconnect { id, task } => match task.poll() {
            None => false,
            Some(Ok(started)) => {
                let succeeded = sessions.finish_reconnect(task.identity(), started);
                flows.actions.acknowledge(*id, succeeded);
                keep = false;
                true
            }
            Some(Err(error)) => fail(flows, *id, error, &mut keep),
        },
    };
    if keep {
        flows.task = Some(task);
    }
    Ok(changed)
}

fn fail(flows: &mut ControlFlows, id: ActionId, error: String, keep: &mut bool) -> bool {
    flows.actions.acknowledge(id, false);
    flows.action_error = Some(error);
    *keep = false;
    true
}

pub(super) fn apply_removed(actions: &mut ShellActionQueue, model: &mut ShellModel) -> bool {
    let removed = actions.take_removed();
    let changed = !removed.is_empty();
    for entry in removed {
        if let Intent::OpenCodex(target) = entry.intent {
            model.finish_codex_open(&target, None, true);
        }
    }
    changed
}
