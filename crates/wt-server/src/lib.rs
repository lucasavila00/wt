pub mod config;
pub mod daemon;
pub mod operations;
pub mod runtime_config;
mod schema;
pub mod service;
pub mod store;

pub use runtime_config::{
    GitConfig, GuestConfig, ImageConfig, InstallConfig, RegistryCacheConfig, ServerConfig,
    ServerLibvirtConfig, SERVER_CONFIG_PATH,
};

use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION};

pub fn handle_request<W: wt_provider::WorldWorker>(
    service: &service::Service<W>,
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
