use super::{map_store_error, AgentGitGateway, Service};
use wt_control_protocol::{ApiError, ErrorCode, InstanceName, InstanceStatus, Response};
use wt_retained_worlds::WorldWorker;

impl<W: WorldWorker, G: AgentGitGateway> Service<W, G> {
    pub(super) fn stop(&self, owner: &str, name: &InstanceName) -> Result<Response, ApiError> {
        let _operation = self
            .operations
            .try_lock(owner, name)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "instance operation is active"))?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        self.reconcile(&stored)?;
        let stored = self.store.get(owner, name).map_err(map_store_error)?;
        if stored.instance.status == InstanceStatus::Stopped {
            return Ok(Response::Instance {
                instance: Box::new(stored.instance),
            });
        }
        if !matches!(
            stored.instance.status,
            InstanceStatus::Setup | InstanceStatus::Running
        ) {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!(
                    "world is {}; expected setup or running",
                    stored.instance.status
                ),
            ));
        }
        self.worker
            .stop(stored.instance.kind(), &stored.backend_id)
            .map_err(|error| ApiError::new(ErrorCode::Backend, format!("stop world: {error}")))?;
        let disk_usage_bytes = self.disk_usage(&stored)?;
        self.store
            .mark_stopped(
                stored.instance.id,
                "guest stopped (requested)",
                disk_usage_bytes,
            )
            .map_err(map_store_error)?;
        let instance = self
            .store
            .get(owner, name)
            .map_err(map_store_error)?
            .instance;
        Ok(Response::Instance {
            instance: Box::new(instance),
        })
    }
}
