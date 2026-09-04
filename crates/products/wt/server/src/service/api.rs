use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{ApiError, ApiResponse, ErrorCode, Operation, Outcome, Response};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub fn execute_api_read(
        &self,
        owner: &str,
        request_id: uuid::Uuid,
        expected_server_id: Option<uuid::Uuid>,
        operation: Operation,
    ) -> ApiResponse {
        let server_id = match self.store.server_id() {
            Ok(server_id) => server_id,
            Err(error) => return ApiResponse::error(map_store_error(error)),
        };
        if expected_server_id.is_some_and(|expected| expected != server_id) {
            return ApiResponse::error(ApiError::new(
                ErrorCode::ServerMismatch,
                "request was addressed to a different WT server",
            ))
            .with_request_metadata(request_id, server_id, None);
        }
        if !matches!(
            operation,
            Operation::ListWorldMail { .. } | Operation::InspectCodex { .. }
        ) {
            return ApiResponse::error(ApiError::new(
                ErrorCode::InvalidRequest,
                "request IDs are supported only for public API operations",
            ))
            .with_request_metadata(request_id, server_id, None);
        }
        match self.execute(owner, operation) {
            Ok(response) => ApiResponse::ok(response),
            Err(error) => ApiResponse::error(error),
        }
        .with_request_metadata(request_id, server_id, None)
    }

    pub fn execute_api_mutation(
        &self,
        owner: &str,
        request_id: uuid::Uuid,
        request_hash: Option<&str>,
        expected_server_id: Option<uuid::Uuid>,
        operation: Operation,
        progress: &mut dyn std::io::Write,
    ) -> ApiResponse {
        let server_id = match self.store.server_id() {
            Ok(server_id) => server_id,
            Err(error) => return ApiResponse::error(map_store_error(error)),
        };
        if expected_server_id.is_some_and(|expected| expected != server_id) {
            return ApiResponse::error(ApiError::new(
                ErrorCode::ServerMismatch,
                "request was addressed to a different WT server",
            ))
            .with_request_metadata(request_id, server_id, None);
        }
        if !matches!(
            operation,
            Operation::CreateWorld(_)
                | Operation::DeleteWorld { .. }
                | Operation::StartCodex { .. }
                | Operation::ResumeCodex { .. }
                | Operation::SendCodexMessage { .. }
        ) {
            return ApiResponse::error(ApiError::new(
                ErrorCode::InvalidRequest,
                "request IDs are supported only for mutating public API operations",
            ))
            .with_request_metadata(request_id, server_id, None);
        }
        let Some(request_hash) = request_hash
            .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        else {
            return ApiResponse::error(ApiError::new(
                ErrorCode::InvalidRequest,
                "request hash must be 64 hexadecimal characters",
            ))
            .with_request_metadata(request_id, server_id, None);
        };
        match self
            .store
            .begin_api_mutation(owner, request_id, request_hash)
        {
            Ok(wt_workload_registry::ApiMutationStart::Replay {
                response_json,
                expires_at_unix_ms,
            }) => match serde_json::from_str::<Outcome>(&response_json) {
                Ok(outcome) => ApiResponse::from_outcome(outcome).with_request_metadata(
                    request_id,
                    server_id,
                    Some(expires_at_unix_ms),
                ),
                Err(error) => ApiResponse::error(ApiError::new(
                    ErrorCode::Internal,
                    format!("decode stored API result: {error}"),
                ))
                .with_request_metadata(request_id, server_id, None),
            },
            Ok(wt_workload_registry::ApiMutationStart::InProgress) => ApiResponse::error(
                ApiError::new(ErrorCode::Conflict, "request is still in progress").retryable(),
            )
            .with_request_metadata(request_id, server_id, None),
            Ok(wt_workload_registry::ApiMutationStart::Conflict) => {
                ApiResponse::error(ApiError::new(
                    ErrorCode::Conflict,
                    "request ID was reused with different content",
                ))
                .with_request_metadata(request_id, server_id, None)
            }
            Ok(wt_workload_registry::ApiMutationStart::Started { expires_at_unix_ms }) => self
                .execute_new_mutation(
                    owner,
                    request_id,
                    request_hash,
                    server_id,
                    expires_at_unix_ms,
                    operation,
                    progress,
                ),
            Err(error) => ApiResponse::error(map_store_error(error).retryable())
                .with_request_metadata(request_id, server_id, None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_new_mutation(
        &self,
        owner: &str,
        request_id: uuid::Uuid,
        request_hash: &str,
        server_id: uuid::Uuid,
        expires_at_unix_ms: i64,
        operation: Operation,
        progress: &mut dyn std::io::Write,
    ) -> ApiResponse {
        let deleted_world_id = match &operation {
            Operation::DeleteWorld { world_id } => Some(*world_id),
            _ => None,
        };
        let codex_mutation = matches!(
            operation,
            Operation::StartCodex { .. }
                | Operation::ResumeCodex { .. }
                | Operation::SendCodexMessage { .. }
        );
        let result = self.execute_with_progress(owner, operation, progress);
        let outcome = match result {
            Ok(response) => Outcome::Ok {
                response: Box::new(response),
            },
            Err(error) if error.code == ErrorCode::NotFound && deleted_world_id.is_some() => {
                Outcome::Ok {
                    response: Box::new(Response::WorldDeleted {
                        world_id: deleted_world_id.expect("checked above"),
                    }),
                }
            }
            Err(mut error) => {
                if !codex_mutation
                    && matches!(
                        error.code,
                        ErrorCode::Capacity | ErrorCode::Backend | ErrorCode::Internal
                    )
                {
                    error.retryable = true;
                }
                Outcome::Error { error }
            }
        };
        if matches!(&outcome, Outcome::Error { error } if error.retryable) {
            let _ = self
                .store
                .abort_api_mutation(owner, request_id, request_hash);
            return ApiResponse::from_outcome(outcome)
                .with_request_metadata(request_id, server_id, None);
        }
        let response_json = match serde_json::to_string(&outcome) {
            Ok(response_json) => response_json,
            Err(error) => {
                if !codex_mutation {
                    let _ = self
                        .store
                        .abort_api_mutation(owner, request_id, request_hash);
                }
                let mut error = ApiError::new(ErrorCode::Internal, error.to_string());
                error.retryable = !codex_mutation;
                return ApiResponse::error(error)
                    .with_request_metadata(request_id, server_id, None);
            }
        };
        if let Err(error) =
            self.store
                .finish_api_mutation(owner, request_id, request_hash, &response_json)
        {
            // Codex execution may already have happened. Preserve its reservation even
            // when storing the response fails; a later retry must not run the turn again.
            if !codex_mutation {
                let _ = self
                    .store
                    .abort_api_mutation(owner, request_id, request_hash);
            }
            let mut error =
                ApiError::new(ErrorCode::Internal, format!("store API result: {error}"));
            error.retryable = !codex_mutation;
            return ApiResponse::error(error).with_request_metadata(request_id, server_id, None);
        }
        ApiResponse::from_outcome(outcome).with_request_metadata(
            request_id,
            server_id,
            Some(expires_at_unix_ms),
        )
    }
}
