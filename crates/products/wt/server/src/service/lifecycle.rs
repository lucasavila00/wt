use super::{map_store_error, AgentToolGrantAuthority, LivePaneObservations, Service};
use wt_control_protocol::{ApiError, ErrorCode, Response, WorldId, WorldStatus};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGrantAuthority + LivePaneObservations> Service<W, G> {
    pub(super) fn stop(&self, owner: &str, world_id: WorldId) -> Result<Response, ApiError> {
        let stored = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "world operation is active"))?;
        self.reconcile_locked(&stored)?;
        let stored = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?;
        if stored.world.status == WorldStatus::Stopped {
            return Ok(Response::World {
                world: Box::new(stored.world),
            });
        }
        if stored.world.status != WorldStatus::Running {
            return Err(ApiError::new(
                ErrorCode::Conflict,
                format!("world is {}; expected running", stored.world.status),
            ));
        }
        self.worker
            .stop(world_id)
            .map_err(|error| ApiError::new(ErrorCode::Backend, format!("stop world: {error}")))?;
        let disk_usage_bytes = self.disk_usage(&stored)?;
        self.store
            .mark_stopped(world_id, "guest stopped (requested)", disk_usage_bytes)
            .map_err(map_store_error)?;
        if let Err(error) = self.gateway.deactivate_pane_observations(world_id) {
            eprintln!("wt-server: deactivate stopped world pane observations: {error}");
        }
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
