use crate::store::{Store, StoreError, StoredInstance};
use uuid::Uuid;
use wt_api::{ApiError, CreateInstance, ErrorCode, Instance, InstanceStatus, Operation, Response};
use wt_libvirt::{ProvisionSpec, WorldWorker};

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
        let id = Uuid::new_v4();
        let backend_id = format!("wt-{}", id.simple());
        let stored = StoredInstance {
            instance: Instance {
                id,
                name: request.name,
                owner: owner.to_owned(),
                status: InstanceStatus::Provisioning,
                guest_ip: None,
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
            Ok(world) => {
                self.store
                    .mark_running(id, &world.guest_ip)
                    .map_err(map_store_error)?;
                let mut instance = stored.instance;
                instance.status = InstanceStatus::Running;
                instance.guest_ip = Some(world.guest_ip);
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

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict => ApiError::new(ErrorCode::Conflict, "instance already exists"),
        StoreError::NotFound => ApiError::new(ErrorCode::NotFound, "instance not found"),
        other => ApiError::new(ErrorCode::Internal, other.to_string()),
    }
}
