use crate::operations::Operations;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use wt_control_protocol::{
    ApiError, Capacity, CapacityResource, CreateApplication, CreateInstance, ErrorCode, Instance,
    InstanceApplication, InstanceStatus, Operation, Response,
};
use wt_retained_worlds::{ProvisionSpec, World, WorldApplication, WorldInspection, WorldWorker};
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
        if let CreateApplication::Devcontainer {
            source, git_base, ..
        } = &request.application
        {
            wt_control_protocol::validate_ssh_git_source(source)
                .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error.to_string()))?;
            wt_control_protocol::validate_git_branch(git_base)
                .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error.to_string()))?;
        }
        if let CreateApplication::Host { user_data } = &request.application {
            wt_retained_worlds::host::validate_user_data(user_data)
                .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        }
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
        let kind = request.kind();
        let grant = match &request.application {
            CreateApplication::Devcontainer {
                source, git_base, ..
            } => self.gateway.reserve(id, Some(source), Some(git_base)),
            CreateApplication::Host { .. } => self.gateway.reserve(id, None, None),
        };
        let grant = Some(grant.map_err(|error| ApiError::new(ErrorCode::Backend, error))?);
        let disk_id = Uuid::new_v4();
        let backend_id = format!("wt-{}", id.simple());
        let (application, stored_application) = match &request.application {
            CreateApplication::Devcontainer {
                source, git_base, ..
            } => (
                InstanceApplication::Devcontainer {
                    source: source.clone(),
                    git_base: git_base.clone(),
                    git_prefix: wt_agent_tool_gateway::BRANCH_PREFIX.to_owned(),
                    app_ssh: None,
                },
                wt_workload_registry::StoredApplication::Devcontainer {
                    gateway_grant_id: grant.as_ref().expect("devcontainer grant").id.clone(),
                },
            ),
            CreateApplication::Host { .. } => (
                InstanceApplication::Host,
                wt_workload_registry::StoredApplication::Host {
                    gateway_grant_id: Some(grant.as_ref().expect("host grant").id.clone()),
                },
            ),
        };
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
                application,
            },
            backend_id,
            disk_id,
            setup_fingerprint,
            application: stored_application,
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

        let spec = match (&request.application, &stored.instance.application) {
            (
                CreateApplication::Devcontainer { .. },
                InstanceApplication::Devcontainer {
                    source,
                    git_base,
                    git_prefix,
                    ..
                },
            ) => ProvisionSpec::Devcontainer(wt_retained_worlds::devcontainer::ProvisionSpec {
                id,
                backend_id: &stored.backend_id,
                disk_id,
                owner,
                name: &stored.instance.name,
                source,
                git_base,
                git_prefix,
                git_grant: &grant.as_ref().expect("devcontainer grant").token,
                git_user_name: &request.git_user_name,
                git_user_email: &request.git_user_email,
                memory_mib: request.memory_mib,
                vcpus: request.vcpus,
                disk_gib: request.disk_gib,
                ssh_authorized_keys: &request.ssh_authorized_keys,
            }),
            (CreateApplication::Host { user_data }, InstanceApplication::Host) => {
                ProvisionSpec::Host(wt_retained_worlds::host::ProvisionSpec {
                    backend_id: &stored.backend_id,
                    disk_id,
                    memory_mib: request.memory_mib,
                    vcpus: request.vcpus,
                    disk_gib: request.disk_gib,
                    ssh_authorized_keys: &request.ssh_authorized_keys,
                    user_data,
                    git_grant: &grant.as_ref().expect("host grant").token,
                    git_user_name: &request.git_user_name,
                    git_user_email: &request.git_user_email,
                })
            }
            _ => unreachable!("request and stored application kinds match"),
        };
        let result = self.worker.provision(spec, &mut std::io::stderr());
        match result {
            Ok(world) => match world.application {
                WorldApplication::Devcontainer { .. } => self
                    .store
                    .mark_setup(id, world.access.guest_ip(), world.access.ssh())
                    .map_err(map_store_error)?,
                WorldApplication::Host { setup_complete } => {
                    if setup_complete {
                        self.store
                            .mark_host_running(id, world.access.guest_ip(), world.access.ssh())
                            .map_err(map_store_error)?
                    } else {
                        self.store
                            .mark_setup(id, world.access.guest_ip(), world.access.ssh())
                            .map_err(map_store_error)?
                    }
                }
            },
            Err(error) => {
                let provisioning_error = error.to_string();
                if kind == wt_control_protocol::WorldKind::Host {
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
                let cleanup = grant
                    .as_ref()
                    .map_or(Ok(()), |grant| {
                        self.gateway
                            .revoke(&grant.id)
                            .map_err(|error| format!("Git grant revocation failed: {error}"))
                    })
                    .and_then(|()| {
                        self.worker
                            .destroy(kind, &stored.backend_id, disk_id)
                            .map_err(|error| format!("world cleanup failed: {error}"))
                    })
                    .and_then(|()| {
                        self.store
                            .delete(id, disk_id)
                            .map_err(|error| format!("registry cleanup failed: {error}"))
                    });
                if let Err(cleanup) = cleanup {
                    let cleanup_error = format!("{provisioning_error}; {cleanup}");
                    eprintln!(
                        "wt-server: failed create cleanup {}: {cleanup}",
                        stored.instance.name
                    );
                    if let Err(store_error) = self.store.mark_error(id, &cleanup_error) {
                        eprintln!(
                            "wt-server: record failed create cleanup {}: {store_error}",
                            stored.instance.name
                        );
                    }
                }
                return Err(ApiError::new(ErrorCode::Backend, provisioning_error));
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
        match self
            .worker
            .inspect(stored.instance.kind(), &stored.backend_id)
        {
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
        self.worker
            .disk_usage(stored.instance.kind(), stored.disk_id)
            .map_err(|error| {
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
        let same_app_identity = match (&stored.instance.application, &world.application) {
            (
                InstanceApplication::Devcontainer {
                    app_ssh: previous, ..
                },
                WorldApplication::Devcontainer { app_ssh: current },
            ) => match (previous, current) {
                (Some(previous), Some(current)) => previous.host_keys == current.host_keys,
                (None, _) => true,
                _ => false,
            },
            (InstanceApplication::Host, WorldApplication::Host { .. }) => true,
            _ => false,
        };
        if !same_guest_identity || !same_app_identity {
            return self
                .store
                .mark_error(stored.instance.id, "SSH host identity changed")
                .map_err(map_store_error);
        }
        match &world.application {
            WorldApplication::Devcontainer {
                app_ssh: Some(app_ssh),
            } => self
                .store
                .mark_running(
                    stored.instance.id,
                    world.access.guest_ip(),
                    world.access.ssh(),
                    app_ssh,
                )
                .map_err(map_store_error),
            WorldApplication::Devcontainer { app_ssh: None } => self
                .store
                .mark_setup(
                    stored.instance.id,
                    world.access.guest_ip(),
                    world.access.ssh(),
                )
                .map_err(map_store_error),
            WorldApplication::Host { setup_complete } => {
                if *setup_complete {
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
        let world = match self
            .worker
            .start(stored.instance.kind(), &stored.backend_id)
        {
            Ok(world) => world,
            Err(error) => {
                if let Ok(WorldInspection::Stopped { reason }) = self
                    .worker
                    .inspect(stored.instance.kind(), &stored.backend_id)
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
        let gateway_grant_id = match &stored.application {
            wt_workload_registry::StoredApplication::Devcontainer { gateway_grant_id } => {
                Some(gateway_grant_id)
            }
            wt_workload_registry::StoredApplication::Host { gateway_grant_id } => {
                gateway_grant_id.as_ref()
            }
        };
        if let Some(gateway_grant_id) = gateway_grant_id {
            self.gateway
                .revoke(gateway_grant_id)
                .map_err(|error| ApiError::new(ErrorCode::Backend, error))?;
        }
        self.store
            .mark_destroying(stored.instance.id)
            .map_err(map_store_error)?;
        if let Err(error) =
            self.worker
                .destroy(stored.instance.kind(), &stored.backend_id, stored.disk_id)
        {
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
        (&instance.application, instance.status),
        (
            InstanceApplication::Devcontainer { .. },
            InstanceStatus::Provisioning | InstanceStatus::Setup
        ) | (
            InstanceApplication::Host,
            InstanceStatus::Provisioning | InstanceStatus::Setup | InstanceStatus::Running
        )
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
