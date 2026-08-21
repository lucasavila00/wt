use crate::operations::Operations;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use wt_control_protocol::{
    ApiError, Capacity, CapacityResource, CreateInstance, ErrorCode, Instance, InstanceStatus,
    Operation, Response,
};
use wt_retained_worlds::{ProvisionSpec, World, WorldInspection, WorldWorker};
use wt_workload_registry::Resources;
use wt_workload_registry::{Store, StoreError, StoredInstance};
mod codex;
mod gateway;
mod lifecycle;
mod reports;
#[cfg(test)]
mod tests;
pub use gateway::AgentToolGateway;
pub struct Service<W, G> {
    store: Store,
    worker: W,
    gateway: G,
    operations: Operations,
    capacity_limit: Resources,
    codex_sessions_path: PathBuf,
}

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub fn new(
        store: Store,
        worker: W,
        gateway: G,
        operations: Operations,
        memory_limit_mib: u64,
    ) -> Self {
        Self {
            store,
            worker,
            gateway,
            operations,
            capacity_limit: Resources {
                memory_mib: memory_limit_mib,
                ..Resources::UNLIMITED
            },
            codex_sessions_path: PathBuf::from(crate::CODEX_SESSIONS_PATH),
        }
    }

    pub fn with_capacity_limit(
        store: Store,
        worker: W,
        gateway: G,
        operations: Operations,
        capacity_limit: Resources,
    ) -> Self {
        Self {
            store,
            worker,
            gateway,
            operations,
            capacity_limit,
            codex_sessions_path: PathBuf::from(crate::CODEX_SESSIONS_PATH),
        }
    }

    pub fn with_codex_sessions_path(mut self, path: impl AsRef<Path>) -> Self {
        self.codex_sessions_path = path.as_ref().to_owned();
        self
    }

    pub fn execute(&self, owner: &str, operation: Operation) -> Result<Response, ApiError> {
        if owner.is_empty() {
            return Err(ApiError::new(ErrorCode::Internal, "process user is empty"));
        }
        match operation {
            Operation::Create(request) => self.create(owner, request),
            Operation::List => self.list(owner),
            Operation::Get { name } => self.get(owner, &name),
            Operation::Start { name } => self.start(owner, &name),
            Operation::Stop { name } => self.stop(owner, &name),
            Operation::Delete { name } => self.delete(owner, &name),
            Operation::ListAgentToolReports => self.list_agent_tool_reports(owner),
            Operation::ClearAgentToolReports => self.clear_agent_tool_reports(owner),
            Operation::ListCodexSessions => self.list_codex_sessions(owner),
        }
    }

    fn create(&self, owner: &str, request: CreateInstance) -> Result<Response, ApiError> {
        wt_control_protocol::validate_create_resources(&request)
            .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        wt_retained_worlds::host::validate_user_data(include_str!(
            "../../../../../assets/client/cloud-init.yaml"
        ))
        .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        let _operation = self.operations.lock(owner, &request.name);
        let setup_fingerprint = setup_fingerprint(&request)?;
        match self.store.get(owner, &request.name) {
            Ok(stored)
                if retryable_create(&stored.instance)
                    && stored.setup_fingerprint == setup_fingerprint =>
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
        for (resource, requested, total, unit) in [
            (
                CapacityResource::Cpu,
                u64::from(request.vcpus),
                self.capacity_limit.vcpus,
                "CPU",
            ),
            (
                CapacityResource::Memory,
                request.memory_mib,
                self.capacity_limit.memory_mib,
                "MiB RAM",
            ),
            (
                CapacityResource::Disk,
                request.disk_gib,
                self.capacity_limit.disk_gib,
                "GiB disk",
            ),
        ] {
            if requested > total {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    format!(
                        "world requests {requested} {unit} but the server {resource} limit is {total}"
                    ),
                ));
            }
        }
        let id = Uuid::new_v4();
        let grant = self.gateway.reserve(id);
        let grant = Some(grant.map_err(|error| ApiError::new(ErrorCode::Backend, error))?);
        let disk_id = Uuid::new_v4();
        let backend_id = format!("wt-{}", id.simple());
        let stored = StoredInstance {
            instance: Instance {
                id,
                name: request.name.clone(),
                owner: owner.to_owned(),
                status: InstanceStatus::Provisioning,
                vcpus: request.vcpus,
                memory_mib: request.memory_mib,
                disk_gib: request.disk_gib,
                guest_ip: None,
                last_error: None,
                ssh: None,
            },
            backend_id,
            disk_id,
            setup_fingerprint,
            gateway_grant_id: Some(grant.as_ref().expect("host grant").id.clone()),
        };
        if let Err(error) = self
            .store
            .insert_with_capacity_limit(&stored, self.capacity_limit)
        {
            if let Some(grant) = &grant {
                if let Err(cleanup) = self.gateway.revoke(&grant.id) {
                    eprintln!("wt-server: revoke unused Git grant: {cleanup}");
                }
            }
            return Err(map_store_error(error));
        }

        let spec = ProvisionSpec {
            backend_id: &stored.backend_id,
            disk_id,
            memory_mib: request.memory_mib,
            vcpus: request.vcpus,
            disk_gib: request.disk_gib,
            ssh_authorized_keys: &request.ssh_authorized_keys,
            git_grant: &grant.as_ref().expect("host grant").token,
            git_user_name: &request.git_user_name,
            git_user_email: &request.git_user_email,
        };
        let result = self.worker.provision(spec, &mut std::io::stderr());
        match result {
            Ok(world) => {
                if world.setup_complete {
                    self.store
                        .mark_host_running(id, world.access.guest_ip(), world.access.ssh())
                        .map_err(map_store_error)?
                } else {
                    self.store
                        .mark_setup(id, world.access.guest_ip(), world.access.ssh())
                        .map_err(map_store_error)?
                }
            }
            Err(error) => {
                let provisioning_error = error.to_string();
                if let Err(store_error) = self.store.mark_error(id, &provisioning_error) {
                    eprintln!(
                        "wt-server: record failed host create {}: {store_error}",
                        stored.instance.name
                    );
                    return Err(ApiError::new(
                        ErrorCode::Backend,
                        format!(
                            "{provisioning_error}; failed to record the retained host world: \
                             {store_error}"
                        ),
                    ));
                }
                eprintln!(
                    "wt-server: retained failed host world {}: {provisioning_error}",
                    stored.instance.name
                );
                return Err(ApiError::new(
                    ErrorCode::Backend,
                    format!(
                        "{provisioning_error}; host world '{}' was retained in error state; \
                         run `wt rm {}` to delete it",
                        stored.instance.name, stored.instance.name
                    ),
                ));
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
        for instance in &stored {
            self.reconcile(instance)?;
        }
        let stored = self.store.list(owner).map_err(map_store_error)?;
        let mut disk_usage_bytes = std::collections::BTreeMap::new();
        for world in &stored {
            let usage = self.disk_usage(world)?;
            disk_usage_bytes.insert(world.instance.id, usage);
        }
        let instances = stored.into_iter().map(|stored| stored.instance).collect();
        let agent_tool_report_counts = self
            .store
            .agent_tool_report_counts(owner)
            .map_err(map_store_error)?;
        Ok(Response::Instances {
            instances,
            disk_usage_bytes,
            agent_tool_report_counts,
        })
    }

    fn reconcile(&self, stored: &StoredInstance) -> Result<(), ApiError> {
        if !matches!(
            stored.instance.status,
            InstanceStatus::Setup | InstanceStatus::Running | InstanceStatus::Stopped
        ) {
            return Ok(());
        }
        match self.worker.inspect(&stored.backend_id) {
            Ok(WorldInspection::Running(world)) => {
                self.store
                    .ensure_resources_reserved(stored.instance.id)
                    .map_err(map_store_error)?;
                self.apply_world(stored, &world)?
            }
            Ok(WorldInspection::Stopped { reason }) => {
                let disk_usage_bytes = self.disk_usage(stored)?;
                self.store
                    .mark_stopped(
                        stored.instance.id,
                        &stopped_message(reason.as_deref()),
                        disk_usage_bytes,
                    )
                    .map_err(map_store_error)?
            }
            Ok(WorldInspection::Missing) => self
                .store
                .mark_error(stored.instance.id, "guest domain is missing")
                .map_err(map_store_error)?,
            Err(error) => self
                .store
                .mark_error(
                    stored.instance.id,
                    &format!("guest reconciliation: {error}"),
                )
                .map_err(map_store_error)?,
        }
        Ok(())
    }

    fn disk_usage(&self, stored: &StoredInstance) -> Result<u64, ApiError> {
        self.worker.disk_usage(stored.disk_id).map_err(|error| {
            ApiError::new(
                ErrorCode::Backend,
                format!("read world disk usage: {error}"),
            )
        })
    }

    fn apply_world(&self, stored: &StoredInstance, world: &World) -> Result<(), ApiError> {
        let same_guest_identity = stored
            .instance
            .ssh
            .as_ref()
            .is_some_and(|ssh| ssh.host_keys == world.access.ssh().host_keys);
        if !same_guest_identity {
            return self
                .store
                .mark_error(stored.instance.id, "SSH host identity changed")
                .map_err(map_store_error);
        }
        if world.setup_complete {
            self.store
                .mark_host_running(
                    stored.instance.id,
                    world.access.guest_ip(),
                    world.access.ssh(),
                )
                .map_err(map_store_error)
        } else {
            self.store
                .mark_setup(
                    stored.instance.id,
                    world.access.guest_ip(),
                    world.access.ssh(),
                )
                .map_err(map_store_error)
        }
    }

    fn get(
        &self,
        owner: &str,
        name: &wt_control_protocol::InstanceName,
    ) -> Result<Response, ApiError> {
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        self.reconcile(&stored)?;
        let instance = self
            .store
            .get(owner, name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance {
            instance: Box::new(instance),
        })
    }

    fn start(
        &self,
        owner: &str,
        name: &wt_control_protocol::InstanceName,
    ) -> Result<Response, ApiError> {
        let _operation = self
            .operations
            .try_lock(owner, name)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "instance operation is active"))?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        self.reconcile(&stored)?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        if matches!(
            stored.instance.status,
            InstanceStatus::Setup | InstanceStatus::Running
        ) {
            return Ok(Response::Instance {
                instance: Box::new(stored.instance),
            });
        }
        if stored.instance.status != InstanceStatus::Stopped {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!("world is {}; expected stopped", stored.instance.status),
            ));
        }
        self.store
            .reserve_resources(stored.instance.id, self.capacity_limit)
            .map_err(map_store_error)?;
        let world = match self.worker.start(&stored.backend_id) {
            Ok(world) => world,
            Err(error) => {
                if let Ok(WorldInspection::Stopped { reason }) =
                    self.worker.inspect(&stored.backend_id)
                {
                    let disk_usage_bytes = self.disk_usage(&stored)?;
                    self.store
                        .mark_stopped(
                            stored.instance.id,
                            &stopped_message(reason.as_deref()),
                            disk_usage_bytes,
                        )
                        .map_err(map_store_error)?;
                }
                return Err(ApiError::new(
                    ErrorCode::Backend,
                    format!("start world: {error}"),
                ));
            }
        };
        self.apply_world(&stored, &world)?;
        let instance = self
            .store
            .get(owner, name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance {
            instance: Box::new(instance),
        })
    }

    fn delete(
        &self,
        owner: &str,
        name: &wt_control_protocol::InstanceName,
    ) -> Result<Response, ApiError> {
        let _operation = self
            .operations
            .try_lock(owner, name)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "instance operation is active"))?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        let gateway_grant_id = stored.gateway_grant_id.as_ref();
        if let Some(gateway_grant_id) = gateway_grant_id {
            self.gateway
                .revoke(gateway_grant_id)
                .map_err(|error| ApiError::new(ErrorCode::Backend, error))?;
        }
        self.store
            .mark_destroying(stored.instance.id)
            .map_err(map_store_error)?;
        if let Err(error) = self.worker.destroy(&stored.backend_id, stored.disk_id) {
            let message = error.to_string();
            self.store
                .mark_error(stored.instance.id, &message)
                .map_err(map_store_error)?;
            return Err(ApiError::new(ErrorCode::Backend, message));
        }
        self.store
            .delete(stored.instance.id, stored.disk_id)
            .map_err(map_store_error)?;
        Ok(Response::Deleted { name: name.clone() })
    }
}

fn retryable_create(instance: &Instance) -> bool {
    matches!(
        instance.status,
        InstanceStatus::Provisioning | InstanceStatus::Setup | InstanceStatus::Running
    )
}

fn setup_fingerprint(request: &CreateInstance) -> Result<String, ApiError> {
    let bytes = serde_json::to_vec(request)
        .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict => ApiError::new(ErrorCode::Conflict, "instance already exists"),
        StoreError::NotFound => ApiError::new(ErrorCode::NotFound, "instance not found"),
        StoreError::Capacity {
            resource,
            total,
            reserved,
            requested,
        } => ApiError::capacity(Capacity {
            resource: match resource {
                wt_workload_registry::Resource::Cpu => CapacityResource::Cpu,
                wt_workload_registry::Resource::Memory => CapacityResource::Memory,
                wt_workload_registry::Resource::Disk => CapacityResource::Disk,
            },
            total,
            reserved,
            requested,
        }),
        other => ApiError::new(ErrorCode::Internal, other.to_string()),
    }
}

fn stopped_message(reason: Option<&str>) -> String {
    reason.map_or_else(
        || "guest stopped".to_owned(),
        |reason| format!("guest stopped ({reason})"),
    )
}
