use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{
    ApiError, ApiResponse, ErrorCode, Operation, Response, WorldMail, WorldName,
    MAX_WORLD_MAIL_PAGE_SIZE,
};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn execute_public_mail(
        &self,
        owner: &str,
        operation: Operation,
        progress: &mut dyn std::io::Write,
    ) -> ApiResponse {
        match self.execute_with_progress(owner, operation, progress) {
            Ok(response) => ApiResponse::ok(response),
            Err(error) => ApiResponse::error(error),
        }
    }

    pub(super) fn list_world_mail(
        &self,
        owner: &str,
        world_id: wt_world::WorldId,
        after_id: u64,
        limit: u32,
    ) -> Result<Response, ApiError> {
        if !(1..=MAX_WORLD_MAIL_PAGE_SIZE).contains(&limit) {
            return Err(ApiError::new(
                ErrorCode::InvalidRequest,
                format!("mail limit must be between 1 and {MAX_WORLD_MAIL_PAGE_SIZE}"),
            ));
        }
        let page = self
            .store
            .list_world_mail(owner, world_id, after_id, limit)
            .map_err(map_store_error)?;
        let messages = page
            .messages
            .into_iter()
            .map(|message| {
                Ok(WorldMail {
                    id: message.id,
                    client_message_id: message.client_message_id,
                    world_id: message.world_id,
                    world_name: WorldName::parse(message.world_name)
                        .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?,
                    window_id: message.window_id,
                    created_at_unix_ms: message.created_at_unix_ms,
                    message: message.message,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::WorldMail {
            messages,
            high_water_id: page.high_water_id,
        })
    }
}
