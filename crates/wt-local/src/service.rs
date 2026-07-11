use crate::store::{Store, StoreError, StoredInstance};
use crate::worker::{ProvisionSpec, WorldWorker};
use uuid::Uuid;
use wt_api::{ApiError, CreateInstance, ErrorCode, Instance, InstanceStatus, Operation, Response};

pub struct Service<W> {
    store: Store,
    worker: W,
}

impl<W: WorldWorker> Service<W> {
    pub fn new(store: Store, worker: W) -> Self {
        Self { store, worker }
    }

    pub fn execute(&mut self, owner: &str, operation: Operation) -> Result<Response, ApiError> {
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
        validate_create(&request)?;
        let id = Uuid::new_v4();
        let backend_id = format!("wt-{}", id.simple());
        let stored = StoredInstance {
            instance: Instance {
                id,
                name: request.name,
                owner: owner.to_owned(),
                source: request.source,
                git_ref: request.git_ref,
                status: InstanceStatus::Provisioning,
                endpoint: None,
                last_error: None,
            },
            backend_id,
        };
        self.store.insert(&stored).map_err(map_store_error)?;

        let spec = ProvisionSpec {
            id,
            backend_id: &stored.backend_id,
            owner,
            name: &stored.instance.name,
        };
        match self.worker.provision(&spec) {
            Ok(endpoint) => {
                self.store
                    .mark_running(id, &endpoint)
                    .map_err(map_store_error)?;
                let mut instance = stored.instance;
                instance.status = InstanceStatus::Running;
                instance.endpoint = Some(endpoint);
                Ok(Response::Instance { instance })
            }
            Err(error) => {
                let message = error.to_string();
                self.store
                    .mark_error(id, &message)
                    .map_err(map_store_error)?;
                Err(ApiError::new(ErrorCode::Backend, message))
            }
        }
    }

    fn list(&self, owner: &str) -> Result<Response, ApiError> {
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
        let instance = self
            .store
            .get(owner, name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance { instance })
    }

    fn delete(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
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
        Ok(Response::Deleted { name: name.clone() })
    }
}

fn validate_create(request: &CreateInstance) -> Result<(), ApiError> {
    if request.source.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "source must not be empty",
        ));
    }
    if request.source.len() > 4096 {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "source is too long",
        ));
    }
    if request
        .git_ref
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "git ref must not be empty",
        ));
    }
    Ok(())
}

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict => ApiError::new(ErrorCode::Conflict, "instance already exists"),
        StoreError::NotFound => ApiError::new(ErrorCode::NotFound, "instance not found"),
        other => ApiError::new(ErrorCode::Internal, other.to_string()),
    }
}
