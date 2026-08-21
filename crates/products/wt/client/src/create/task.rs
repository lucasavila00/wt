use anyhow::{Context as _, Result};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use wt_client::config::ClientConfig;
use wt_control_protocol::{ApiRequest, CreateInstance, Operation, Outcome, Response};

use super::{capacity_message, Created, Input};

pub(super) enum TaskEvent {
    Capacity(String),
    Finished(Result<Box<Created>, String>),
}

pub(super) struct Task {
    events: Receiver<TaskEvent>,
    retry: Sender<bool>,
}

impl Task {
    pub(super) fn start(config: &ClientConfig, input: Input) -> Result<Self> {
        let context = config
            .context(&input.context)
            .context("selected context is missing")?
            .clone();
        let context_name = context.name.clone();
        let request = CreateInstance {
            name: input.name,
            vcpus: input.vcpus,
            memory_mib: input.memory_mib,
            disk_gib: input.disk_gib,
            ssh_authorized_keys: input.ssh_authorized_keys,
            git_user_name: input.git_user_name,
            git_user_email: input.git_user_email,
            application: input.application,
        };
        let config = config.clone();
        let (event_sender, events) = mpsc::channel();
        let (retry, retries) = mpsc::channel();
        thread::Builder::new()
            .name("wt-create-world".into())
            .spawn(move || {
                run(
                    config,
                    context,
                    context_name,
                    request,
                    &event_sender,
                    &retries,
                )
            })
            .context("start world creation task")?;
        Ok(Self { events, retry })
    }

    pub(super) fn poll(&self) -> Option<TaskEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(TaskEvent::Finished(Err(
                "world creation task stopped unexpectedly".into(),
            ))),
        }
    }

    pub(super) fn retry(&self, retry: bool) {
        let _ = self.retry.send(retry);
    }
}

fn run(
    config: ClientConfig,
    context: wt_client::config::Context,
    context_name: String,
    request: CreateInstance,
    events: &Sender<TaskEvent>,
    retries: &Receiver<bool>,
) {
    loop {
        let outcome = match wt_client::transport::call_outcome(
            &context,
            &ApiRequest::new(Operation::Create(request.clone())),
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                finish(
                    events,
                    Err(format!(
                        "create did not complete; run `wt ls` to check the world: {error:#}"
                    )),
                );
                return;
            }
        };
        match outcome {
            Outcome::Ok { response } => {
                let Response::Instance { instance } = *response else {
                    finish(
                        events,
                        Err("helper returned the wrong response to create".into()),
                    );
                    return;
                };
                let instance = *instance;
                if let Err(error) = crate::sync_complete_inventory(&config).with_context(|| {
                    format!(
                        "world {}.{} was created, but setup was not entered\nresolve the synchronization error, run `wt sync`, and reconnect with `ssh {}.{}`",
                        context_name, instance.name, context_name, instance.name
                    )
                }) {
                    finish(events, Err(format!("{error:#}")));
                    return;
                }
                finish(
                    events,
                    Ok(Created {
                        context: context_name,
                        instance,
                    }),
                );
                return;
            }
            Outcome::Error { error } if error.code == wt_control_protocol::ErrorCode::Capacity => {
                let Some(capacity) = error.capacity.as_ref() else {
                    finish(
                        events,
                        Err("server returned a capacity error without capacity details".into()),
                    );
                    return;
                };
                if events
                    .send(TaskEvent::Capacity(capacity_message(
                        &context_name,
                        &request.name,
                        capacity,
                    )))
                    .is_err()
                    || retries.recv() != Ok(true)
                {
                    return;
                }
            }
            Outcome::Error { error } => {
                let error = wt_client::transport::rejection(&context, &error);
                finish(
                    events,
                    Err(format!(
                        "create did not complete; run `wt ls` to check the world: {error:#}"
                    )),
                );
                return;
            }
        }
    }
}

fn finish(events: &Sender<TaskEvent>, result: Result<Created, String>) {
    let _ = events.send(TaskEvent::Finished(result.map(Box::new)));
}
