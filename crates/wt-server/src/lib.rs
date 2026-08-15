pub mod client;
pub mod config;
pub mod daemon;
pub mod operations;
pub mod runtime_config;
mod schema;
pub mod service;
pub mod store;
pub mod worlds;

pub use runtime_config::{
    AgentGitConfig, AgentGitProviderConfig, GuestConfig, ImageConfig, InstallConfig,
    RegistryCacheConfig, ServerConfig, ServerLibvirtConfig, SERVER_CONFIG_PATH,
};

use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION};

pub fn handle_request<W: worlds::WorldWorker, G: service::AgentGitGateway>(
    service: &service::Service<W, G>,
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
