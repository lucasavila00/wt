extern crate self as wt_server;

#[path = "main.rs"]
mod command;
pub mod config;
pub mod daemon;
pub mod identity;
pub mod image_generation;
pub mod operations;
pub mod runtime_config;
pub mod service;
pub mod shared_files;

pub use command::run_from;

pub use identity::{
    validate_process_identity, validate_shared_roots, SERVER_GID, SERVER_GROUP, SERVER_HOME,
    SERVER_UID, SERVER_USER,
};
pub use runtime_config::{
    AgentToolsConfig, AgentToolsProviderConfig, CodexPaths, GuestConfig, ImageConfig,
    InstallConfig, ServerConfig, ServerLibvirtConfig, AGENT_TOOL_VSOCK_PORT_ENV, CODEX_AUTH_PATH,
    CODEX_AUTH_SHARE_DIR, CODEX_SESSIONS_PATH, DEFAULT_AGENT_TOOL_VSOCK_PORT, SERVER_CONFIG_PATH,
    SSH_AUTHORIZED_KEYS_SHARE_DIR, TEST_CODEX_AUTH_PATH, TEST_CODEX_AUTH_SHARE_DIR,
    TEST_CODEX_SESSIONS_PATH,
};

use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION};

pub fn handle_request<W: wt_guest::WorldWorker, G: service::AgentToolGateway>(
    service: &service::Service<W, G>,
    owner: &str,
    request: ApiRequest,
) -> ApiResponse {
    handle_request_with_progress(service, owner, request, false, &mut std::io::sink())
}

pub fn handle_request_with_progress<W: wt_guest::WorldWorker, G: service::AgentToolGateway>(
    service: &service::Service<W, G>,
    owner: &str,
    request: ApiRequest,
    test_server: bool,
    progress: &mut dyn std::io::Write,
) -> ApiResponse {
    if let Err(error) = validate_protocol_version(request.protocol_version) {
        return ApiResponse::error(error);
    }

    if request.operation == wt_control_protocol::Operation::ServerInfo {
        return ApiResponse::ok(wt_control_protocol::Response::ServerInfo {
            test_server,
            build: wt_control_protocol::BuildIdentity::current(),
        });
    }

    match service.execute_with_progress(owner, request.operation, progress) {
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
            @"unsupported protocol version 12; expected 11"
        );
    }
}
