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
    RegistryCacheConfig, ServerConfig, ServerLibvirtConfig, AGENT_GIT_VSOCK_PORT_ENV,
    CODEX_AUTH_PATH, CODEX_AUTH_SHARE_DIR, CODEX_SESSIONS_PATH, DEFAULT_AGENT_GIT_VSOCK_PORT,
    SERVER_CONFIG_PATH,
};

use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION};

pub fn handle_request<W: worlds::WorldWorker, G: service::AgentGitGateway>(
    service: &service::Service<W, G>,
    owner: &str,
    request: ApiRequest,
) -> ApiResponse {
    if let Err(error) = validate_protocol_version(request.protocol_version) {
        return ApiResponse::error(error);
    }

    match service.execute(owner, request.operation) {
        Ok(response) => ApiResponse::ok(response),
        Err(error) => ApiResponse::error(error),
    }
}

fn validate_protocol_version(protocol_version: u32) -> Result<(), ApiError> {
    if protocol_version == PROTOCOL_VERSION {
        return Ok(());
    }
    Err(ApiError::new(
        ErrorCode::UnsupportedProtocol,
        format!("unsupported protocol version {protocol_version}; expected {PROTOCOL_VERSION}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_supported_protocol_version() {
        assert_eq!(validate_protocol_version(PROTOCOL_VERSION), Ok(()));
    }

    #[test]
    fn rejects_an_unsupported_protocol_version() {
        let error = validate_protocol_version(PROTOCOL_VERSION + 1).unwrap_err();
        insta::assert_snapshot!(
            error.message,
            @"unsupported protocol version 3; expected 2"
        );
    }
}
