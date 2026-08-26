use crate::operations::Operations;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use wt_control_protocol::{
    ApiError, Capacity, CapacityResource, CreateWorld, ErrorCode, Operation, ResourceCapacity,
    Resources as ProtocolResources, Response, World, WorldId, WorldName, WorldStatus,
};
use wt_guest::{GuestAccess, WorldInspection, WorldProvisionSpec, WorldWorker};
use wt_workload_registry::Resources;
use wt_workload_registry::{Store, StoreError, StoredWorld};
mod activity;
mod codex_catalog;
mod gateway;
mod lifecycle;
mod pane;
mod reports;
#[cfg(test)]
mod tests;
pub use gateway::AgentToolGateway;

pub fn refresh_codex_session_catalog(store: &Store, root: &Path) -> Result<Vec<String>, String> {
    codex_catalog::refresh(store, root)
}

const INSPECTION_RETRIES: usize = 6;
const INSPECTION_RETRY_DELAY: Duration = Duration::from_secs(10);

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
        self.execute_with_progress(owner, operation, &mut std::io::sink())
    }

    pub fn execute_with_progress(
        &self,
        owner: &str,
        operation: Operation,
        progress: &mut dyn std::io::Write,
    ) -> Result<Response, ApiError> {
        if owner.is_empty() {
            return Err(ApiError::new(ErrorCode::Internal, "process user is empty"));
        }
        match operation {
            Operation::ServerInfo => unreachable!("server info is handled before service dispatch"),
            Operation::CreateWorld(request) => self.create(owner, request, progress),
            Operation::ListWorlds => self.list(owner),
            Operation::GetWorld { name } => self.get(owner, &name),
            Operation::RenameWorld { world_id, new_name } => {
                self.rename(owner, world_id, &new_name)
            }
            Operation::StartWorld { world_id } => self.start(owner, world_id),
            Operation::StopWorld { world_id } => self.stop(owner, world_id),
            Operation::DeleteWorld { world_id } => self.delete(owner, world_id),
            Operation::ListAgentToolReports => self.list_agent_tool_reports(owner),
            Operation::ClearAgentToolReports => self.clear_agent_tool_reports(owner),
            Operation::ListPaneObservations => self.list_pane_observations(owner),
            Operation::ListGitActivity { query } => self.list_git_activity(owner, query),
            Operation::ListWtToolsActivity { query } => self.list_wt_tools_activity(owner, query),
        }
    }

    fn create(
        &self,
        owner: &str,
        request: CreateWorld,
        progress: &mut dyn std::io::Write,
    ) -> Result<Response, ApiError> {
        wt_control_protocol::validate_create_world_resources(&request)
            .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        let _operation = self.operations.lock_create(&request.name);
        let setup_fingerprint = setup_fingerprint(&request)?;
        match self.store.get_owned_by_name(owner, &request.name) {
            Ok(stored)
                if retryable_create(&stored.world)
                    && stored.setup_fingerprint == setup_fingerprint =>
            {
                return Ok(Response::World {
                    world: Box::new(stored.world),
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
        let world_id = WorldId::new();
        let grant = self.gateway.reserve(world_id);
        let grant = Some(grant.map_err(|error| ApiError::new(ErrorCode::Backend, error))?);
        let stored = StoredWorld {
            world: World {
                world_id,
                name: request.name.clone(),
                owner: owner.to_owned(),
                status: WorldStatus::Provisioning,
                vcpus: request.vcpus,
                memory_mib: request.memory_mib,
                disk_gib: request.disk_gib,
                guest_ip: None,
                last_error: None,
                ssh: None,
            },
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

        let spec = WorldProvisionSpec {
            world_id,
            memory_mib: request.memory_mib,
            vcpus: request.vcpus,
            disk_gib: request.disk_gib,
            git_grant: &grant.as_ref().expect("host grant").token,
            git_user_name: &request.git_user_name,
            git_user_email: &request.git_user_email,
        };
        let result = self.worker.provision(spec, progress);
        match result {
            Ok(access) => self
                .store
                .mark_host_running(world_id, access.guest_ip(), access.ssh())
                .map_err(map_store_error)?,
            Err(error) => {
                let provisioning_error = error.to_string();
                if let Err(store_error) = self.store.mark_error(world_id, &provisioning_error) {
                    eprintln!(
                        "wt-server: record failed host create {}: {store_error}",
                        stored.world.name
                    );
                    return Err(ApiError::new(
                        ErrorCode::Backend,
                        format!(
                            "{provisioning_error}; failed to record the guest: \
                             {store_error}"
                        ),
                    ));
                }
                eprintln!(
                    "wt-server: preserved failed guest {}: {provisioning_error}",
                    stored.world.name
                );
                return Err(ApiError::new(
                    ErrorCode::Backend,
                    format!(
                        "{provisioning_error}; guest '{}' was preserved in error state; \
                         run `wt rm {}` to delete it",
                        stored.world.name, stored.world.name
                    ),
                ));
            }
        }
        let world = self
            .store
            .get_owned_by_id(owner, stored.world.world_id)
            .map_err(map_store_error)?
            .world;
        Ok(Response::World {
            world: Box::new(world),
        })
    }

    fn list(&self, owner: &str) -> Result<Response, ApiError> {
        let stored = self.store.list_owned(owner).map_err(map_store_error)?;
        for world in &stored {
            self.reconcile(world)?;
        }
        let stored = self.store.list_owned(owner).map_err(map_store_error)?;
        let mut disk_usage_bytes = std::collections::BTreeMap::new();
        for world in &stored {
            let usage = self.disk_usage(world)?;
            disk_usage_bytes.insert(world.world.world_id, usage);
        }
        let worlds = stored.into_iter().map(|stored| stored.world).collect();
        let agent_tool_report_counts = self
            .store
            .agent_tool_report_counts(owner)
            .map_err(map_store_error)?;
        let reserved = self.store.reserved_resources().map_err(map_store_error)?;
        Ok(Response::Worlds {
            worlds,
            capacity: ResourceCapacity {
                reserved: protocol_resources(reserved),
                total: protocol_resources(self.capacity_limit),
            },
            disk_usage_bytes,
            agent_tool_report_counts,
        })
    }

    fn reconcile(&self, stored: &StoredWorld) -> Result<(), ApiError> {
        if !matches!(
            stored.world.status,
            WorldStatus::Running | WorldStatus::Stopped | WorldStatus::Error
        ) {
            return Ok(());
        }
        let retries = if stored.world.status == WorldStatus::Error {
            0
        } else {
            INSPECTION_RETRIES
        };
        match retry(
            || self.worker.inspect(stored.world.world_id),
            retries,
            || std::thread::sleep(INSPECTION_RETRY_DELAY),
        ) {
            Ok(WorldInspection::Running(world)) => {
                self.store
                    .ensure_resources_reserved(stored.world.world_id)
                    .map_err(map_store_error)?;
                self.apply_world(stored, &world)?
            }
            Ok(WorldInspection::Stopped { reason }) => {
                let disk_usage_bytes = self.disk_usage(stored)?;
                self.store
                    .mark_stopped(
                        stored.world.world_id,
                        &stopped_message(reason.as_deref()),
                        disk_usage_bytes,
                    )
                    .map_err(map_store_error)?
            }
            Ok(WorldInspection::Missing) => self
                .store
                .mark_error(stored.world.world_id, "guest domain is missing")
                .map_err(map_store_error)?,
            Err(error) => self
                .store
                .mark_error(
                    stored.world.world_id,
                    &format!("guest reconciliation: {error}"),
                )
                .map_err(map_store_error)?,
        }
        Ok(())
    }

    fn disk_usage(&self, stored: &StoredWorld) -> Result<u64, ApiError> {
        self.worker
            .disk_usage(stored.world.world_id)
            .map_err(|error| {
                ApiError::new(
                    ErrorCode::Backend,
                    format!("read world disk usage: {error}"),
                )
            })
    }

    fn apply_world(&self, stored: &StoredWorld, access: &GuestAccess) -> Result<(), ApiError> {
        let same_guest_identity = stored
            .world
            .ssh
            .as_ref()
            .is_some_and(|ssh| ssh.host_keys == access.ssh().host_keys);
        if !same_guest_identity {
            return self
                .store
                .mark_error(stored.world.world_id, "SSH host identity changed")
                .map_err(map_store_error);
        }
        self.store
            .mark_host_running(stored.world.world_id, access.guest_ip(), access.ssh())
            .map_err(map_store_error)
    }

    fn get(&self, owner: &str, name: &WorldName) -> Result<Response, ApiError> {
        let stored = self
            .store
            .get_owned_by_name(owner, name)
            .map_err(map_store_error)?;
        self.reconcile(&stored)?;
        let world = self
            .store
            .get_owned_by_id(owner, stored.world.world_id)
            .map_err(map_store_error)?
            .world;
        Ok(Response::World {
            world: Box::new(world),
        })
    }

    fn start(&self, owner: &str, world_id: WorldId) -> Result<Response, ApiError> {
        let stored = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "world operation is active"))?;
        self.reconcile(&stored)?;
        let stored = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        if matches!(stored.world.status, WorldStatus::Running) {
            return Ok(Response::World {
                world: Box::new(stored.world),
            });
        }
        if stored.world.status != WorldStatus::Stopped {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!("world is {}; expected stopped", stored.world.status),
            ));
        }
        self.store
            .reserve_resources(world_id, self.capacity_limit)
            .map_err(map_store_error)?;
        let access = match self.worker.start(world_id) {
            Ok(world) => world,
            Err(error) => {
                if let Ok(WorldInspection::Stopped { reason }) = self.worker.inspect(world_id) {
                    let disk_usage_bytes = self.disk_usage(&stored)?;
                    self.store
                        .mark_stopped(
                            world_id,
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
        self.apply_world(&stored, &access)?;
        let world = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?
            .world;
        Ok(Response::World {
            world: Box::new(world),
        })
    }

    fn delete(&self, owner: &str, world_id: WorldId) -> Result<Response, ApiError> {
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "world operation is active"))?;
        let stored = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        let gateway_grant_id = stored.gateway_grant_id.as_ref();
        if let Some(gateway_grant_id) = gateway_grant_id {
            self.gateway
                .revoke(gateway_grant_id)
                .map_err(|error| ApiError::new(ErrorCode::Backend, error))?;
        }
        self.store
            .mark_destroying(world_id)
            .map_err(map_store_error)?;
        if let Err(error) = self.worker.destroy(world_id) {
            let message = error.to_string();
            self.store
                .mark_error(world_id, &message)
                .map_err(map_store_error)?;
            return Err(ApiError::new(ErrorCode::Backend, message));
        }
        self.store.delete(world_id).map_err(map_store_error)?;
        Ok(Response::WorldDeleted { world_id })
    }

    fn rename(
        &self,
        owner: &str,
        world_id: WorldId,
        new_name: &WorldName,
    ) -> Result<Response, ApiError> {
        self.store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "world operation is active"))?;
        self.store
            .rename(world_id, new_name)
            .map_err(map_store_error)?;
        let world = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?
            .world;
        Ok(Response::World {
            world: Box::new(world),
        })
    }
}

fn protocol_resources(resources: Resources) -> ProtocolResources {
    ProtocolResources {
        vcpus: resources.vcpus,
        memory_mib: resources.memory_mib,
        disk_gib: resources.disk_gib,
    }
}

fn retry<T, E>(
    mut operation: impl FnMut() -> Result<T, E>,
    retries: usize,
    mut wait: impl FnMut(),
) -> Result<T, E> {
    let mut attempts = 0;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if attempts == retries => return Err(error),
            Err(_) => {
                attempts += 1;
                wait();
            }
        }
    }
}

fn retryable_create(world: &World) -> bool {
    matches!(
        world.status,
        WorldStatus::Provisioning | WorldStatus::Running
    )
}

fn setup_fingerprint(request: &CreateWorld) -> Result<String, ApiError> {
    let bytes = serde_json::to_vec(request)
        .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn map_store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict => ApiError::new(ErrorCode::Conflict, "world name already exists"),
        StoreError::NotFound => ApiError::new(ErrorCode::NotFound, "world not found"),
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
