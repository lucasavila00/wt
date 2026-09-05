use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{ApiError, ErrorCode, ExecCommand, Response, WorldId, WorldStatus};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn exec_world(
        &self,
        owner: &str,
        world_id: WorldId,
        command: &ExecCommand,
    ) -> Result<Response, ApiError> {
        if !command.executable.starts_with('/')
            || command.executable.contains('\0')
            || command.args.iter().any(|arg| arg.contains('\0'))
            || command.args.len() > 256
            || command.stdin.len() + command.args.iter().map(String::len).sum::<usize>()
                > 1024 * 1024
        {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                "invalid bounded command",
            ));
        }
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "world has an active operation"))?;
        let world = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?
            .world;
        if world.status != WorldStatus::Running {
            return Err(ApiError::new(ErrorCode::Conflict, "world is not running"));
        }
        let output = self.worker.exec_world(world_id, command).map_err(|error| {
            ApiError::new(
                ErrorCode::Internal,
                format!("command outcome unknown; do not replay automatically: {error}"),
            )
        })?;
        Ok(Response::WorldExecuted { output })
    }
}
