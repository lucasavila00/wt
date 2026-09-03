use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{
    ApiError, ErrorCode, Operation, Response, WorldMail, MAX_WORLD_MAIL_PAGE_SIZE,
};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_world_mail(
        &self,
        owner: &str,
        operation: Operation,
    ) -> Result<Response, ApiError> {
        let Operation::ListWorldMail {
            world_id,
            after_id,
            limit,
        } = operation
        else {
            unreachable!("mail dispatch requires a mail operation")
        };
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
        Ok(Response::WorldMail {
            messages: page
                .messages
                .into_iter()
                .map(|mail| WorldMail {
                    id: mail.id,
                    world_id: mail.world_id,
                    created_at_unix_ms: mail.created_at_unix_ms,
                    message: mail.message,
                })
                .collect(),
            high_water_id: page.high_water_id,
        })
    }
}
