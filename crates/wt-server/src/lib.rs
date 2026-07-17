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

use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode, PROTOCOL_VERSION, WT_GIT_COMMIT};

pub fn handle_request<W: wt_provider::WorldWorker>(
    service: &service::Service<W>,
    owner: &str,
    request: ApiRequest,
) -> ApiResponse {
    if let Err(error) = validate_client_commit(&request.client_commit) {
        return ApiResponse::error(error);
    }
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

fn validate_client_commit(client_commit: &str) -> Result<(), ApiError> {
    if client_commit == WT_GIT_COMMIT {
        return Ok(());
    }
    Err(ApiError::new(
        ErrorCode::UnsupportedProtocol,
        format!(
            "client commit {client_commit} does not match server commit {WT_GIT_COMMIT}; install `wt` and `wt-server` built from the same commit"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_server_commit() {
        assert_eq!(validate_client_commit(WT_GIT_COMMIT), Ok(()));
    }

    #[test]
    fn mismatch_reports_both_commits() {
        let error = validate_client_commit("0000000000000000000000000000000000000000").unwrap_err();
        insta::assert_snapshot!(
            error.message.replace(WT_GIT_COMMIT, "[SERVER_COMMIT]"),
            @"client commit 0000000000000000000000000000000000000000 does not match server commit [SERVER_COMMIT]; install `wt` and `wt-server` built from the same commit"
        );
    }
}
