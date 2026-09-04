use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{
    ApiError, CodexMessageDelivery, CodexStatus, ErrorCode, Response, WorldId, WorldStatus,
    MAX_MAIL_TEXT_BYTES,
};
use wt_guest::{CodexRuntimeStatus, WorldWorker};

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn start_codex(
        &self,
        owner: &str,
        world_id: WorldId,
        message: &str,
    ) -> Result<Response, ApiError> {
        validate_message(message)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(active_operation)?;
        self.require_running_world(owner, world_id)?;
        let started = self
            .worker
            .start_codex(world_id, message)
            .map_err(worker_error)?;
        Ok(Response::CodexStarted {
            thread_id: started.thread_id,
            turn_id: started.turn_id,
            pane_id: started.pane_id,
            window_name: started.window_name,
        })
    }

    pub(super) fn inspect_codex(
        &self,
        owner: &str,
        world_id: WorldId,
        thread_id: &str,
    ) -> Result<Response, ApiError> {
        self.codex_inspection(owner, world_id, thread_id, false)
    }

    pub(super) fn resume_codex(
        &self,
        owner: &str,
        world_id: WorldId,
        thread_id: &str,
    ) -> Result<Response, ApiError> {
        self.codex_inspection(owner, world_id, thread_id, true)
    }

    fn codex_inspection(
        &self,
        owner: &str,
        world_id: WorldId,
        thread_id: &str,
        resume: bool,
    ) -> Result<Response, ApiError> {
        validate_thread_id(thread_id)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(active_operation)?;
        self.require_running_world(owner, world_id)?;
        let inspection = if resume {
            self.worker.resume_codex(world_id, thread_id)
        } else {
            self.worker.inspect_codex(world_id, thread_id)
        }
        .map_err(worker_error)?;
        Ok(Response::CodexInspection {
            thread_id: thread_id.to_owned(),
            status: match inspection.status {
                CodexRuntimeStatus::Active => CodexStatus::Active,
                CodexRuntimeStatus::Idle => CodexStatus::Idle,
                CodexRuntimeStatus::Error => CodexStatus::Error,
            },
            active_turn_id: inspection.active_turn_id,
            pane_id: inspection.pane_id,
            window_name: inspection.window_name,
            screen: inspection.screen,
            observed_at_unix_ms: inspection.observed_at_unix_ms,
        })
    }

    pub(super) fn send_codex_message(
        &self,
        owner: &str,
        world_id: WorldId,
        thread_id: &str,
        message: &str,
    ) -> Result<Response, ApiError> {
        validate_thread_id(thread_id)?;
        validate_message(message)?;
        let _operation = self
            .operations
            .try_lock_world(world_id)
            .ok_or_else(active_operation)?;
        self.require_running_world(owner, world_id)?;
        let sent = self
            .worker
            .send_codex_message(world_id, thread_id, message)
            .map_err(worker_error)?;
        Ok(Response::CodexMessageSent {
            thread_id: thread_id.to_owned(),
            turn_id: sent.turn_id,
            delivery: match sent.delivery {
                wt_guest::CodexMessageDelivery::Steered => CodexMessageDelivery::Steered,
                wt_guest::CodexMessageDelivery::Started => CodexMessageDelivery::Started,
            },
        })
    }

    fn require_running_world(&self, owner: &str, world_id: WorldId) -> Result<(), ApiError> {
        let world = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?
            .world;
        if world.status != WorldStatus::Running {
            return Err(ApiError::new(ErrorCode::Conflict, "world is not running"));
        }
        Ok(())
    }
}

fn validate_message(message: &str) -> Result<(), ApiError> {
    if message.is_empty() || message.len() > MAX_MAIL_TEXT_BYTES {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("message must contain 1 to {MAX_MAIL_TEXT_BYTES} UTF-8 bytes"),
        ));
    }
    Ok(())
}

fn validate_thread_id(thread_id: &str) -> Result<(), ApiError> {
    if thread_id.is_empty() {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "thread ID must not be empty",
        ));
    }
    Ok(())
}

fn active_operation() -> ApiError {
    ApiError::new(ErrorCode::Conflict, "world operation is active").retryable()
}

fn worker_error(error: wt_libvirt_kvm::WorkerError) -> ApiError {
    // Codex may have accepted work before a response was lost. Do not release the
    // mutation reservation and blindly submit it again with the same request ID.
    ApiError::new(
        ErrorCode::Backend,
        format!("{error}; inspect the thread before submitting new work"),
    )
}
