use crate::operations::Operations;
use crate::store::{Store, StoreError, StoredInstance};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wt_api::{
    ApiError, Capacity, CapacityResource, CreateApplication, CreateInstance, ErrorCode, Instance,
    InstanceApplication, InstanceStatus, Operation, Response,
};
use wt_devcontainer::{World, WorldInspection, WorldWorker};
use wt_registry::Resources;

pub trait AgentGitGateway {
    fn reserve(
        &self,
        world_id: Uuid,
        source: &str,
        base: &str,
    ) -> Result<wt_devcontainer_git::Grant, String>;
    fn revoke(&self, grant_id: &str) -> Result<(), String>;
}

impl AgentGitGateway for wt_devcontainer_git::ControlClient {
    fn reserve(
        &self,
        world_id: Uuid,
        source: &str,
        base: &str,
    ) -> Result<wt_devcontainer_git::Grant, String> {
        let response = self
            .request(&wt_devcontainer_git::ControlRequest::Reserve {
                world_id: world_id.to_string(),
                source: source.to_owned(),
                base: base.to_owned(),
            })
            .map_err(|error| error.to_string())?;
        if response.ok {
            response
                .grant
                .ok_or_else(|| "gateway reserve response has no grant".to_owned())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "gateway rejected grant".to_owned()))
        }
    }

    fn revoke(&self, grant_id: &str) -> Result<(), String> {
        let response = self
            .request(&wt_devcontainer_git::ControlRequest::Revoke {
                grant_id: grant_id.to_owned(),
            })
            .map_err(|error| error.to_string())?;
        if response.ok {
            Ok(())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "gateway rejected revocation".to_owned()))
        }
    }
}

pub struct Service<W, G> {
    store: Store,
    worker: W,
    gateway: G,
    operations: Operations,
    capacity_limit: Resources,
}

impl<W: WorldWorker, G: AgentGitGateway> Service<W, G> {
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
        }
    }

    pub fn execute(&self, owner: &str, operation: Operation) -> Result<Response, ApiError> {
        if owner.is_empty() {
            return Err(ApiError::new(ErrorCode::Internal, "process user is empty"));
        }
        match operation {
            Operation::Create(request) => self.create(owner, request),
            Operation::Fork(_) => Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "worlds cannot be forked",
            )),
            Operation::List => self.list(owner),
            Operation::Get { name } => self.get(owner, &name),
            Operation::Start { name } => self.start(owner, &name),
            Operation::Delete { name } => self.delete(owner, &name),
        }
    }

    fn create(&self, owner: &str, request: CreateInstance) -> Result<Response, ApiError> {
        let CreateApplication::Devcontainer {
            source,
            git_base,
            git_user_name,
            git_user_email,
        } = &request.application
        else {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "host world provisioning is unavailable",
            ));
        };
        if let Err(error) = wt_api::validate_ssh_git_source(source) {
            return Err(ApiError::new(ErrorCode::InvalidRequest, error.to_string()));
        }
        wt_api::validate_git_branch(git_base)
            .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error.to_string()))?;
        wt_api::validate_create_resources(&request)
            .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error))?;
        let _operation = self.operations.lock(owner, &request.name);
        let setup_fingerprint = setup_fingerprint(&request)?;
        match self.store.get(owner, &request.name) {
            Ok(stored)
                if stored.instance.status == InstanceStatus::Provisioning
                    && stored.setup_fingerprint == setup_fingerprint =>
            {
                return Ok(Response::Instance {
                    instance: Box::new(stored.instance),
                });
            }
            Ok(stored)
                if stored.instance.status == InstanceStatus::Setup
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
        let git_prefix = wt_devcontainer_git::BRANCH_PREFIX.to_owned();
        let grant = self
            .gateway
            .reserve(id, source, git_base)
            .map_err(|error| ApiError::new(ErrorCode::Backend, error))?;
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
                application: InstanceApplication::Devcontainer {
                    source: source.clone(),
                    git_base: git_base.clone(),
                    git_prefix,
                    app_ssh: None,
                },
            },
            backend_id,
            head_disk_id: disk_id,
            setup_fingerprint,
            application: crate::store::StoredApplication::Devcontainer {
                gateway_grant_id: grant.id.clone(),
            },
        };
        if let Err(error) = self
            .store
            .insert_with_capacity_limit(&stored, self.capacity_limit)
        {
            if let Err(cleanup) = self.gateway.revoke(&grant.id) {
                eprintln!("wt-server: revoke unused Git grant: {cleanup}");
            }
            return Err(map_store_error(error));
        }

        let InstanceApplication::Devcontainer {
            source,
            git_base,
            git_prefix,
            ..
        } = &stored.instance.application
        else {
            unreachable!("created a devcontainer instance")
        };
        let spec = wt_devcontainer::ProvisionSpec {
            id,
            backend_id: &stored.backend_id,
            disk_id,
            owner,
            name: &stored.instance.name,
            source,
            git_base,
            git_prefix,
            git_grant: &grant.token,
            git_user_name,
            git_user_email,
            memory_mib: request.memory_mib,
            vcpus: request.vcpus,
            disk_gib: request.disk_gib,
            ssh_authorized_keys: &request.ssh_authorized_keys,
        };
        let result = self.worker.provision(&spec, &mut std::io::stderr());
        match result {
            Ok(world) => self
                .store
                .mark_setup(id, &world.guest_ip, &world.ssh)
                .map_err(map_store_error)?,
            Err(error) => {
                let provisioning_error = error.to_string();
                let cleanup = self
                    .gateway
                    .revoke(&grant.id)
                    .map_err(|error| format!("Git grant revocation failed: {error}"))
                    .and_then(|()| {
                        self.worker
                            .destroy(&stored.backend_id, &[disk_id])
                            .map_err(|error| format!("world cleanup failed: {error}"))
                    })
                    .and_then(|()| {
                        self.store
                            .delete(id, &[disk_id])
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
        let instances = self
            .store
            .list(owner)
            .map_err(map_store_error)?
            .into_iter()
            .map(|stored| stored.instance)
            .collect();
        Ok(Response::Instances { instances })
    }

    fn reconcile(&self, stored: &StoredInstance) -> Result<(), ApiError> {
        if !matches!(
            stored.instance.status,
            InstanceStatus::Setup | InstanceStatus::Running | InstanceStatus::Stopped
        ) {
            return Ok(());
        }
        match self.worker.inspect(&stored.backend_id) {
            Ok(WorldInspection::Running(world)) => self.apply_world(stored, &world)?,
            Ok(WorldInspection::Stopped { reason }) => self
                .store
                .mark_stopped(stored.instance.id, &stopped_message(reason.as_deref()))
                .map_err(map_store_error)?,
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

    fn apply_world(&self, stored: &StoredInstance, world: &World) -> Result<(), ApiError> {
        let same_guest_identity = stored
            .instance
            .ssh
            .as_ref()
            .is_some_and(|ssh| ssh.host_keys == world.ssh.host_keys);
        let same_app_identity = match (stored.instance.application.app_ssh(), &world.app_ssh) {
            (Some(previous), Some(current)) => previous.host_keys == current.host_keys,
            (None, _) => true,
            _ => false,
        };
        if !same_guest_identity || !same_app_identity {
            return self
                .store
                .mark_error(stored.instance.id, "SSH host identity changed")
                .map_err(map_store_error);
        }
        if let Some(app_ssh) = &world.app_ssh {
            self.store
                .mark_running(stored.instance.id, &world.guest_ip, &world.ssh, app_ssh)
                .map_err(map_store_error)
        } else {
            self.store
                .mark_setup(stored.instance.id, &world.guest_ip, &world.ssh)
                .map_err(map_store_error)
        }
    }

    fn get(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
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

    fn start(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
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
        let reserved = self.store.reserved_resources().map_err(map_store_error)?;
        for (resource, reserved, total) in [
            (
                CapacityResource::Cpu,
                reserved.vcpus,
                self.capacity_limit.vcpus,
            ),
            (
                CapacityResource::Memory,
                reserved.memory_mib,
                self.capacity_limit.memory_mib,
            ),
            (
                CapacityResource::Disk,
                reserved.disk_gib,
                self.capacity_limit.disk_gib,
            ),
        ] {
            if reserved > total {
                return Err(ApiError::capacity(Capacity {
                    resource,
                    total,
                    reserved,
                    requested: 0,
                }));
            }
        }
        let world = self
            .worker
            .start(&stored.backend_id)
            .map_err(|error| ApiError::new(ErrorCode::Backend, format!("start world: {error}")))?;
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

    fn delete(&self, owner: &str, name: &wt_api::InstanceName) -> Result<Response, ApiError> {
        let _operation = self
            .operations
            .try_lock(owner, name)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "instance operation is active"))?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        if let crate::store::StoredApplication::Devcontainer { gateway_grant_id } =
            &stored.application
        {
            self.gateway
                .revoke(gateway_grant_id)
                .map_err(|error| ApiError::new(ErrorCode::Backend, error))?;
        }
        self.store
            .mark_destroying(stored.instance.id)
            .map_err(map_store_error)?;
        let garbage = self
            .store
            .garbage_for_delete(stored.instance.id)
            .map_err(map_store_error)?;
        if let Err(error) = self.worker.destroy(&stored.backend_id, &garbage) {
            let message = error.to_string();
            self.store
                .mark_error(stored.instance.id, &message)
                .map_err(map_store_error)?;
            return Err(ApiError::new(ErrorCode::Backend, message));
        }
        self.store
            .delete(stored.instance.id, &garbage)
            .map_err(map_store_error)?;
        Ok(Response::Deleted { name: name.clone() })
    }
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
                wt_registry::Resource::Cpu => CapacityResource::Cpu,
                wt_registry::Resource::Memory => CapacityResource::Memory,
                wt_registry::Resource::Disk => CapacityResource::Disk,
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

#[cfg(test)]
mod tests {
    use super::*;
    use wt_api::{CreateApplication, InstanceName};

    #[test]
    fn setup_fingerprint_does_not_store_host_user_data() {
        let secret = "token-that-must-not-be-stored";
        let request = CreateInstance {
            name: InstanceName::parse("host").unwrap(),
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            ssh_authorized_keys: vec!["ssh-ed25519 AAAATEST".into()],
            application: CreateApplication::Host {
                user_data: format!("#cloud-config\nwrite_files:\n  - content: {secret}\n"),
            },
        };

        let fingerprint = setup_fingerprint(&request).unwrap();
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!fingerprint.contains(secret));
    }
}
