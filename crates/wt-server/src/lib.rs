pub mod config;
pub mod jobs;
pub mod service;
pub mod store;

use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION};

pub fn handle_request<W: wt_libvirt::WorldWorker>(
    service: &mut service::Service<W>,
    owner: &str,
    request: ApiRequest,
) -> ApiResponse {
    if request.protocol_version != PROTOCOL_VERSION {
        return ApiResponse::error(ApiError::new(
            ErrorCode::UnsupportedProtocol,
            format!(
                "unsupported protocol version {}; expected {}",
                request.protocol_version, PROTOCOL_VERSION
            ),
        ));
    }

    match service.execute(owner, request.operation) {
        Ok(response) => ApiResponse::ok(response),
        Err(error) => ApiResponse::error(error),
    }
}
