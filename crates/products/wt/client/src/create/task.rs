use anyhow::{Context as _, Result};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;
use wt_client::config::ClientConfig;
use wt_control_protocol::{ApiRequest, CreateInstance, Operation, Outcome, Response};

use super::{capacity_message, Created, Input};

const SSH_SYNC_ATTEMPTS: usize = 5;
const SSH_SYNC_RETRY_DELAY: Duration = Duration::from_secs(1);

pub(super) enum TaskEvent {
    Progress(String),
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
            git_user_name: input.git_user_name,
            git_user_email: input.git_user_email,
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
        let outcome = match wt_client::transport::call_outcome_with_progress(
            &context,
            &ApiRequest::new(Operation::Create(request.clone())),
            |message| {
                let _ = events.send(TaskEvent::Progress(message));
            },
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
                let _ = events.send(TaskEvent::Progress(
                    "World created; opening SSH access".into(),
                ));
                if let Err(error) = sync_inventory_after_create(&config, events).with_context(|| {
                    format!(
                        "world {}.{} was created, but SSH was not opened\nresolve the synchronization error, run `wt sync`, and reconnect with `ssh {}.{}`",
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
                {
                    return;
                }
                if retries.recv() != Ok(true) {
                    finish(events, Err("world creation cancelled".into()));
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

fn sync_inventory_after_create(config: &ClientConfig, events: &Sender<TaskEvent>) -> Result<()> {
    retry_database_lock(
        || crate::sync_complete_inventory(config).map(|_| ()),
        |attempt| {
            let _ = events.send(TaskEvent::Progress(format!(
                "SSH inventory is busy; retrying ({attempt}/{SSH_SYNC_ATTEMPTS})"
            )));
            thread::sleep(SSH_SYNC_RETRY_DELAY);
        },
    )
}

fn retry_database_lock<T>(
    mut operation: impl FnMut() -> Result<T>,
    mut retrying: impl FnMut(usize),
) -> Result<T> {
    for attempt in 1..=SSH_SYNC_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error)
                if attempt < SSH_SYNC_ATTEMPTS
                    && error.to_string().contains("database is locked") =>
            {
                retrying(attempt + 1);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the final retry either succeeds or returns its error")
}

fn finish(events: &Sender<TaskEvent>, result: Result<Created, String>) {
    let _ = events.send(TaskEvent::Finished(result.map(Box::new)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_a_database_lock_until_the_inventory_syncs() {
        let mut calls = 0;
        let mut retries = Vec::new();

        let result = retry_database_lock(
            || -> Result<()> {
                calls += 1;
                if calls < 3 {
                    anyhow::bail!("database is locked");
                }
                Ok(())
            },
            |attempt| retries.push(attempt),
        );

        assert!(result.is_ok());
        assert_eq!(calls, 3);
        assert_eq!(retries, [2, 3]);
    }

    #[test]
    fn does_not_retry_a_non_retryable_inventory_error() {
        let mut calls = 0;
        let mut retries = Vec::new();

        let error = retry_database_lock(
            || -> Result<()> {
                calls += 1;
                anyhow::bail!("context helper is unavailable")
            },
            |attempt| retries.push(attempt),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "context helper is unavailable");
        assert_eq!(calls, 1);
        assert!(retries.is_empty());
    }

    #[test]
    fn limits_database_lock_retries() {
        let mut calls = 0;
        let mut retries = Vec::new();

        let error = retry_database_lock(
            || -> Result<()> {
                calls += 1;
                anyhow::bail!("database is locked")
            },
            |attempt| retries.push(attempt),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "database is locked");
        assert_eq!(calls, SSH_SYNC_ATTEMPTS);
        assert_eq!(retries, [2, 3, 4, 5]);
    }
}
