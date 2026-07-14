use crate::jobs::Jobs;
use crate::store::{Store, StoreError, StoredInstance};
use uuid::Uuid;
use wt_api::{ApiError, CreateInstance, ErrorCode, Instance, InstanceStatus, Operation, Response};
use wt_provider::WorldWorker;

pub struct Service<W> {
    store: Store,
    worker: W,
    jobs: Jobs,
}

impl<W: WorldWorker> Service<W> {
    pub fn new(store: Store, worker: W, jobs: Jobs) -> Self {
        Self {
            store,
            worker,
            jobs,
        }
    }

    pub fn execute(&mut self, owner: &str, operation: Operation) -> Result<Response, ApiError> {
        self.jobs.reconcile(&self.store).map_err(map_store_error)?;
        if owner.is_empty() {
            return Err(ApiError::new(ErrorCode::Internal, "process user is empty"));
        }
        match operation {
            Operation::Create(request) => self.create(owner, request),
            Operation::List => self.list(owner),
            Operation::Get { name } => self.get(owner, &name),
            Operation::Delete { name } => self.delete(owner, &name),
        }
    }

    fn create(&self, owner: &str, request: CreateInstance) -> Result<Response, ApiError> {
        if let Err(error) = wt_api::validate_ssh_git_source(&request.source) {
            return Err(ApiError::new(ErrorCode::InvalidRequest, error.to_string()));
        }
        if request.git_branch.is_some() && request.git_ref.is_some() {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "Git branch and ref are mutually exclusive",
            ));
        }
        match self.store.get(owner, &request.name) {
            Ok(stored)
                if matches!(
                    stored.instance.status,
                    InstanceStatus::Provisioning | InstanceStatus::Setup
                ) && stored.instance.source == request.source =>
            {
                return Ok(Response::Instance {
                    instance: Box::new(stored.instance),
                });
            }
            Ok(_) => {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    "instance already exists with different setup inputs or state",
                ));
            }
            Err(StoreError::NotFound) => {}
            Err(error) => return Err(map_store_error(error)),
        }
        let id = Uuid::new_v4();
        let backend_id = format!("wt-{}", id.simple());
        let stored = StoredInstance {
            instance: Instance {
                id,
                name: request.name,
                owner: owner.to_owned(),
                status: InstanceStatus::Provisioning,
                source: request.source,
                guest_ip: None,
                last_error: None,
                ssh: None,
                app_ssh: None,
            },
            backend_id,
        };
        let lock = self
            .jobs
            .lock(id)
            .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
        if let Err(error) = self.store.insert(&stored) {
            drop(lock);
            let _ = self.jobs.remove(id);
            return Err(map_store_error(error));
        }

        let spec = wt_provider::ProvisionSpec {
            id,
            backend_id: &stored.backend_id,
            owner,
            name: &stored.instance.name,
            source: &stored.instance.source,
            git_branch: request.git_branch.as_deref(),
            git_ref: request.git_ref.as_deref(),
            git_user_name: &request.git_user_name,
            git_user_email: &request.git_user_email,
        };
        let result = self.worker.provision(&spec, &mut std::io::sink());
        drop(lock);
        self.jobs
            .remove(id)
            .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
        match result {
            Ok(world) => self
                .store
                .mark_setup(id, &world.guest_ip, &world.ssh)
                .map_err(map_store_error)?,
            Err(error) => {
                let _ = self.worker.destroy(&stored.backend_id);
                let _ = self.store.delete(id);
                return Err(ApiError::new(ErrorCode::Backend, error.to_string()));
            }
        }
        let instance = self
            .store
            .get(owner, &stored.instance.name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance {
            instance: Box::new(instance),
        })
    }

    fn list(&self, owner: &str) -> Result<Response, ApiError> {
        let stored = self.store.list(owner).map_err(map_store_error)?;
        for instance in stored.iter().filter(|item| {
            matches!(
                item.instance.status,
                InstanceStatus::Setup | InstanceStatus::Running
            )
        }) {
            match self.worker.inspect(&instance.backend_id) {
                Ok(Some(world)) => {
                    let same_guest_identity = instance
                        .instance
                        .ssh
                        .as_ref()
                        .is_some_and(|ssh| ssh.host_keys == world.ssh.host_keys);
                    let same_app_identity = match (&instance.instance.app_ssh, &world.app_ssh) {
                        (Some(stored), Some(current)) => stored.host_keys == current.host_keys,
                        (None, _) => true,
                        _ => false,
                    };
                    if same_guest_identity && same_app_identity {
                        if let Some(app_ssh) = &world.app_ssh {
                            self.store
                                .mark_running(
                                    instance.instance.id,
                                    &world.guest_ip,
                                    &world.ssh,
                                    app_ssh,
                                )
                                .map_err(map_store_error)?;
                        } else {
                            self.store
                                .mark_setup(instance.instance.id, &world.guest_ip, &world.ssh)
                                .map_err(map_store_error)?;
                        }
                    } else {
                        self.store
                            .mark_error(instance.instance.id, "SSH host identity changed")
                            .map_err(map_store_error)?;
                    }
                }
                Ok(None) => self
                    .store
                    .mark_error(instance.instance.id, "guest domain is missing")
                    .map_err(map_store_error)?,
                Err(error) => self
                    .store
                    .mark_error(
                        instance.instance.id,
                        &format!("guest reconciliation: {error}"),
                    )
                    .map_err(map_store_error)?,
            }
        }
        let instances = self
            .store
            .list(owner)
            .map_err(map_store_error)?
            .into_iter()
            .map(|stored| stored.instance)
            .collect();
        Ok(Response::Instances { instances })
    }

    fn get(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
        let _ = self.list(owner)?;
        let instance = self
            .store
            .get(owner, name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance {
            instance: Box::new(instance),
        })
    }

    fn delete(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        if stored.instance.status == InstanceStatus::Provisioning
            && self
                .jobs
                .is_locked(stored.instance.id)
                .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?
        {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                "instance is still provisioning",
            ));
        }
        let lock = self
            .jobs
            .lock(stored.instance.id)
            .map_err(|_| ApiError::new(ErrorCode::Conflict, "instance job is active"))?;
        self.store
            .mark_destroying(stored.instance.id)
            .map_err(map_store_error)?;
        if let Err(error) = self.worker.destroy(&stored.backend_id) {
            let message = error.to_string();
            self.store
                .mark_error(stored.instance.id, &message)
                .map_err(map_store_error)?;
            return Err(ApiError::new(ErrorCode::Backend, message));
        }
        self.store
            .delete(stored.instance.id)
            .map_err(map_store_error)?;
        drop(lock);
        self.jobs
            .remove(stored.instance.id)
            .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
        Ok(Response::Deleted { name: name.clone() })
    }
}

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict => ApiError::new(ErrorCode::Conflict, "instance already exists"),
        StoreError::NotFound => ApiError::new(ErrorCode::NotFound, "instance not found"),
        other => ApiError::new(ErrorCode::Internal, other.to_string()),
    }
}
