use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{
    ApiError, ErrorCode, MailKind, Response, WorldId, WorldStatus, MAX_MAIL_TEXT_BYTES,
};
use wt_guest::{CodexTurnOutput, CodexTurnRequest, WorldWorker};

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn run_codex_turn(
        &self,
        owner: &str,
        world_id: WorldId,
        session_id: Option<uuid::Uuid>,
        message: &str,
        request_id: uuid::Uuid,
    ) -> Result<Response, ApiError> {
        if message.is_empty() || message.len() > MAX_MAIL_TEXT_BYTES {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                format!("message must contain 1 to {MAX_MAIL_TEXT_BYTES} UTF-8 bytes"),
            ));
        }
        let _operation = self.operations.try_lock_world(world_id).ok_or_else(|| {
            ApiError::new(ErrorCode::Conflict, "world operation is active").retryable()
        })?;
        let world = self
            .store
            .get_owned_by_id(owner, world_id)
            .map_err(map_store_error)?
            .world;
        if world.status != WorldStatus::Running {
            return Err(ApiError::new(ErrorCode::Conflict, "world is not running"));
        }
        let output = self
            .worker
            .run_codex_turn(
                world_id,
                CodexTurnRequest {
                    message,
                    session_id,
                },
            )
            .unwrap_or_else(|error| CodexTurnOutput {
                session_id,
                result: Err(error.to_string()),
            });
        let (kind, text) = match output.result {
            Ok(text) => (MailKind::Completed, text),
            Err(text) => (MailKind::Failed, text),
        };
        let registry_kind = match kind {
            MailKind::Completed => wt_workload_registry::MailKind::Completed,
            MailKind::Failed => wt_workload_registry::MailKind::Failed,
            MailKind::Message => unreachable!(),
        };
        let mail = self
            .store
            .insert_codex_result(
                world_id,
                request_id,
                output.session_id,
                registry_kind,
                &text,
            )
            .map_err(map_store_error)?;
        Ok(Response::CodexTurn {
            session_id: output.session_id,
            message_id: mail.id,
            kind,
        })
    }
}
