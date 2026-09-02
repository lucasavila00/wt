use anyhow::{bail, Context as _, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read as _, Write as _};
use uuid::Uuid;
use wt_control_protocol::{
    ApiRequest, CapacityResource, CreateWorld, ErrorCode, Operation, Outcome, Response,
};

mod window;
mod world;
use window::ApiWindow;
use world::ApiWorld;

const API_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[rustfmt::skip]
enum Request {
    CreateWorld {
        #[serde(flatten)]
        routing: Routing,
        name: String,
        vcpus: u32,
        memory_mib: u64,
        disk_gib: u64,
        git_user_name: String,
        git_user_email: String,
    },
    DeleteWorld {
        #[serde(flatten)]
        routing: Routing,
        world_id: String,
    },
    StartWindow { #[serde(flatten)] routing: Routing, world_id: String, argv: Vec<String>, cwd: String },
    GetWindow { #[serde(flatten)] routing: Routing, window_id: String, after: u64, limit: u32, include_screen: bool },
    SendWindowInput { #[serde(flatten)] routing: Routing, window_id: String, control_token: String, data_base64: String },
    StopWindow { #[serde(flatten)] routing: Routing, window_id: String, control_token: String },
    DeleteWindow { #[serde(flatten)] routing: Routing, window_id: String, control_token: String },
    ListWorldMail { #[serde(flatten)] routing: Routing, world_id: String, after_id: u64, limit: u32 },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Routing {
    api_version: u32,
    request_id: String,
    #[serde(default)]
    expected_server_id: Option<String>,
    context: String,
}

impl Request {
    fn api_version(&self) -> u32 {
        self.routing().api_version
    }

    fn request_id(&self) -> &str {
        &self.routing().request_id
    }

    fn expected_server_id(&self) -> Option<&str> {
        self.routing().expected_server_id.as_deref()
    }

    fn routing(&self) -> &Routing {
        match self {
            Self::CreateWorld { routing, .. }
            | Self::DeleteWorld { routing, .. }
            | Self::StartWindow { routing, .. }
            | Self::GetWindow { routing, .. }
            | Self::SendWindowInput { routing, .. }
            | Self::StopWindow { routing, .. }
            | Self::DeleteWindow { routing, .. }
            | Self::ListWorldMail { routing, .. } => routing,
        }
    }
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at_unix_ms: Option<i64>,
    #[serde(flatten)]
    outcome: ApiOutcome,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum ApiOutcome {
    Ok { result: ApiResult },
    Error { error: ApiError },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
#[rustfmt::skip]
enum ApiResult {
    World { world: ApiWorld },
    WorldDeleted {
        world_id: String,
    },
    WindowStarted {
        window: ApiWindow,
        control_token: String,
    },
    Window {
        window: ApiWindow,
    },
    WindowInputAccepted {
        window_id: String,
        sequence_id: u64,
    },
    WindowStopped {
        window_id: String,
    },
    WindowDeleted {
        window_id: String,
    },
    WorldMail {
        messages: Vec<ApiWorldMail>,
        high_water_id: u64,
    },
}

#[derive(Debug, Serialize)]
struct ApiWorldMail {
    id: u64,
    client_message_id: String,
    world_id: String,
    world_name: String,
    window_id: String,
    created_at_unix_ms: i64,
    message: String,
}

impl From<wt_control_protocol::WorldMail> for ApiWorldMail {
    fn from(mail: wt_control_protocol::WorldMail) -> Self {
        Self {
            id: mail.id,
            client_message_id: mail.client_message_id.to_string(),
            world_id: mail.world_id.to_string(),
            world_name: mail.world_name.to_string(),
            window_id: mail.window_id.to_string(),
            created_at_unix_ms: mail.created_at_unix_ms,
            message: mail.message,
        }
    }
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<ApiErrorDetails>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ApiErrorDetails {
    Capacity {
        resource: ApiCapacityResource,
        total: u64,
        reserved: u64,
        requested: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiCapacityResource {
    Cpu,
    Memory,
    Disk,
}

impl From<CapacityResource> for ApiCapacityResource {
    fn from(resource: CapacityResource) -> Self {
        match resource {
            CapacityResource::Cpu => Self::Cpu,
            CapacityResource::Memory => Self::Memory,
            CapacityResource::Disk => Self::Disk,
        }
    }
}

struct Reply {
    request_id: Option<String>,
    server_id: Option<String>,
    expires_at_unix_ms: Option<i64>,
    outcome: ApiOutcome,
}

pub fn run() -> Result<()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut input)
        .context("read API request")?;
    if input.len() as u64 > MAX_REQUEST_BYTES {
        return write_local_error(
            None,
            "invalid_request",
            format!("API request exceeds {MAX_REQUEST_BYTES} bytes"),
        );
    }
    let request = match serde_json::from_slice::<Request>(&input) {
        Ok(request) => request,
        Err(error) => return write_parse_error(error),
    };
    let request_id_text = request.request_id().to_owned();
    if request.api_version() != API_VERSION {
        return write_local_error(
            Some(request_id_text),
            "unsupported_api_version",
            format!(
                "unsupported API version {}; expected {API_VERSION}",
                request.api_version()
            ),
        );
    }
    let request_id = match request_id_text.parse::<Uuid>() {
        Ok(request_id) => request_id,
        Err(_) => {
            return write_local_error(
                Some(request_id_text),
                "invalid_request",
                "invalid request ID".to_owned(),
            )
        }
    };
    let expected_server_id = match request.expected_server_id().map(str::parse::<Uuid>) {
        Some(Ok(server_id)) => Some(server_id),
        Some(Err(_)) => {
            return write_local_error(
                Some(request_id.to_string()),
                "invalid_request",
                "invalid expected server ID".to_owned(),
            )
        }
        None => None,
    };
    let (context_name, operation) = match request_to_operation(request) {
        Ok(request) => request,
        Err(message) => {
            return write_local_error(Some(request_id.to_string()), "invalid_request", message)
        }
    };
    let request_hash = operation_hash(&operation);
    let config = match wt_client::config::ClientConfig::load() {
        Ok(config) => config,
        Err(error) => {
            return write_local_error(
                Some(request_id.to_string()),
                "configuration_error",
                format!("{error:#}"),
            )
        }
    };
    let Some(context) = config.context(&context_name) else {
        return write_local_error(
            Some(request_id.to_string()),
            "unknown_context",
            "unknown context".to_owned(),
        );
    };
    finish(call(
        context,
        request_id,
        request_hash,
        expected_server_id,
        operation,
    ))
}

fn operation_hash(operation: &Operation) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(operation).expect("control operation serializes"))
    )
}

fn request_to_operation(request: Request) -> std::result::Result<(String, Operation), String> {
    match request {
        Request::CreateWorld {
            routing,
            name,
            vcpus,
            memory_mib,
            disk_gib,
            git_user_name,
            git_user_email,
            ..
        } => {
            let name = wt_control_protocol::WorldName::parse(name)
                .map_err(|_| "invalid world name".to_owned())?;
            if git_user_name.is_empty() || git_user_email.is_empty() {
                return Err("git_user_name and git_user_email must not be empty".to_owned());
            }
            Ok((
                routing.context,
                Operation::CreateWorld(CreateWorld {
                    name,
                    vcpus,
                    memory_mib,
                    disk_gib,
                    git_user_name,
                    git_user_email,
                }),
            ))
        }
        Request::DeleteWorld {
            routing, world_id, ..
        } => {
            let world_id = world_id
                .parse()
                .map_err(|_| "invalid world ID".to_owned())?;
            Ok((routing.context, Operation::DeleteWorld { world_id }))
        }
        Request::StartWindow {
            routing,
            world_id,
            argv,
            cwd,
            ..
        } => {
            let world_id = world_id
                .parse()
                .map_err(|_| "invalid world ID".to_owned())?;
            let request = wt_control_protocol::StartWindow {
                world_id,
                argv,
                cwd,
                window_id: None,
                control_token: None,
            };
            request.validate()?;
            Ok((routing.context, Operation::StartWindow(request)))
        }
        Request::GetWindow {
            routing,
            window_id,
            after,
            limit,
            include_screen,
            ..
        } => {
            let window_id = window_id
                .parse()
                .map_err(|_| "invalid window ID".to_owned())?;
            Ok((
                routing.context,
                Operation::GetWindow {
                    window_id,
                    after,
                    limit,
                    include_screen,
                },
            ))
        }
        Request::SendWindowInput {
            routing,
            window_id,
            control_token,
            data_base64,
            ..
        } => {
            let window_id = window_id
                .parse()
                .map_err(|_| "invalid window ID".to_owned())?;
            if control_token.is_empty() {
                return Err("control token must not be empty".into());
            }
            let data = base64::engine::general_purpose::STANDARD
                .decode(&data_base64)
                .map_err(|_| "data_base64 is not valid canonical base64".to_owned())?;
            if base64::engine::general_purpose::STANDARD.encode(&data) != data_base64 {
                return Err("data_base64 is not valid canonical base64".to_owned());
            }
            Ok((
                routing.context,
                Operation::SendWindowInput {
                    window_id,
                    control_token,
                    data,
                    api_request_id: None,
                },
            ))
        }
        Request::StopWindow {
            routing,
            window_id,
            control_token,
            ..
        } => {
            let window_id = window_id
                .parse()
                .map_err(|_| "invalid window ID".to_owned())?;
            if control_token.is_empty() {
                return Err("control token must not be empty".into());
            }
            Ok((
                routing.context,
                Operation::StopWindow {
                    window_id,
                    control_token,
                },
            ))
        }
        Request::DeleteWindow {
            routing,
            window_id,
            control_token,
            ..
        } => {
            let window_id = window_id
                .parse()
                .map_err(|_| "invalid window ID".to_owned())?;
            if control_token.is_empty() {
                return Err("control token must not be empty".into());
            }
            Ok((
                routing.context,
                Operation::DeleteWindow {
                    window_id,
                    control_token,
                },
            ))
        }
        Request::ListWorldMail {
            routing,
            world_id,
            after_id,
            limit,
            ..
        } => {
            let world_id = world_id
                .parse()
                .map_err(|_| "invalid world ID".to_owned())?;
            if !(1..=wt_control_protocol::MAX_WORLD_MAIL_PAGE_SIZE).contains(&limit) {
                return Err(format!(
                    "limit must be between 1 and {}",
                    wt_control_protocol::MAX_WORLD_MAIL_PAGE_SIZE
                ));
            }
            Ok((
                routing.context,
                Operation::ListWorldMail {
                    world_id,
                    after_id,
                    limit,
                },
            ))
        }
    }
}

fn call(
    context: &wt_client::config::Context,
    request_id: Uuid,
    request_hash: String,
    expected_server_id: Option<Uuid>,
    operation: Operation,
) -> Reply {
    let response = match wt_client::transport::call_response_with_progress(
        context,
        &ApiRequest::with_request_id(operation, request_id, request_hash, expected_server_id),
        |_| {},
    ) {
        Ok(response) => response,
        Err(error) => {
            return Reply {
                request_id: Some(request_id.to_string()),
                server_id: None,
                expires_at_unix_ms: None,
                outcome: api_error("context_error", error.to_string(), true, None),
            }
        }
    };
    if response.request_id != Some(request_id) || response.server_id.is_none() {
        return Reply {
            request_id: Some(request_id.to_string()),
            server_id: response.server_id.map(|server_id| server_id.to_string()),
            expires_at_unix_ms: None,
            outcome: api_error(
                "internal_error",
                "server omitted or changed API request metadata",
                false,
                None,
            ),
        };
    }
    Reply {
        request_id: Some(request_id.to_string()),
        server_id: response.server_id.map(|server_id| server_id.to_string()),
        expires_at_unix_ms: response.expires_at_unix_ms,
        outcome: match response.outcome {
            Outcome::Ok { response } => match *response {
                Response::World { world } => ApiOutcome::Ok {
                    result: ApiResult::World {
                        world: (*world).into(),
                    },
                },
                Response::WorldDeleted { world_id } => ApiOutcome::Ok {
                    result: ApiResult::WorldDeleted {
                        world_id: world_id.to_string(),
                    },
                },
                Response::WindowStarted {
                    window,
                    control_token,
                } => ApiOutcome::Ok {
                    result: ApiResult::WindowStarted {
                        window: (*window).into(),
                        control_token,
                    },
                },
                Response::Window { window } => ApiOutcome::Ok {
                    result: ApiResult::Window {
                        window: (*window).into(),
                    },
                },
                Response::WindowInputAccepted {
                    window_id,
                    sequence_id,
                } => ApiOutcome::Ok {
                    result: ApiResult::WindowInputAccepted {
                        window_id: window_id.to_string(),
                        sequence_id,
                    },
                },
                Response::WindowStopped { window_id } => ApiOutcome::Ok {
                    result: ApiResult::WindowStopped {
                        window_id: window_id.to_string(),
                    },
                },
                Response::WindowDeleted { window_id } => ApiOutcome::Ok {
                    result: ApiResult::WindowDeleted {
                        window_id: window_id.to_string(),
                    },
                },
                Response::WorldMail {
                    messages,
                    high_water_id,
                } => ApiOutcome::Ok {
                    result: ApiResult::WorldMail {
                        messages: messages.into_iter().map(Into::into).collect(),
                        high_water_id,
                    },
                },
                _ => api_error(
                    "internal_error",
                    "server returned an unexpected response",
                    false,
                    None,
                ),
            },
            Outcome::Error { error } => {
                let details = error.capacity.map(|capacity| ApiErrorDetails::Capacity {
                    resource: capacity.resource.into(),
                    total: capacity.total,
                    reserved: capacity.reserved,
                    requested: capacity.requested,
                });
                api_error(
                    error_code(error.code),
                    error.message,
                    error.retryable,
                    details,
                )
            }
        },
    }
}

fn api_error(
    code: &'static str,
    message: impl Into<String>,
    retryable: bool,
    details: Option<ApiErrorDetails>,
) -> ApiOutcome {
    ApiOutcome::Error {
        error: ApiError {
            code,
            message: message.into(),
            retryable,
            details,
        },
    }
}

fn error_code(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidRequest => "invalid_request",
        ErrorCode::UnsupportedProtocol => "unsupported_protocol",
        ErrorCode::ServerMismatch => "server_mismatch",
        ErrorCode::Conflict => "conflict",
        ErrorCode::NotFound => "not_found",
        ErrorCode::Capacity => "capacity",
        ErrorCode::Backend => "backend_error",
        ErrorCode::Internal => "internal_error",
    }
}

fn write_local_error(
    request_id: Option<String>,
    code: &'static str,
    message: String,
) -> Result<()> {
    eprintln!("wt api: {message}");
    write_response(Reply {
        request_id,
        server_id: None,
        expires_at_unix_ms: None,
        outcome: api_error(code, message, false, None),
    })?;
    bail!("API request failed")
}

fn write_parse_error(parse_error: serde_json::Error) -> Result<()> {
    eprintln!("wt api: invalid JSON request: {parse_error}");
    write_response(Reply {
        request_id: None,
        server_id: None,
        expires_at_unix_ms: None,
        outcome: api_error("invalid_request", "invalid JSON request", false, None),
    })?;
    bail!("API request failed")
}

fn finish(reply: Reply) -> Result<()> {
    let failed = matches!(reply.outcome, ApiOutcome::Error { .. });
    write_response(reply)?;
    if failed {
        eprintln!("wt api: request failed");
        bail!("API request failed")
    }
    Ok(())
}

fn write_response(reply: Reply) -> Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(
        &mut output,
        &ApiResponse {
            api_version: API_VERSION,
            request_id: reply.request_id,
            server_id: reply.server_id,
            expires_at_unix_ms: reply.expires_at_unix_ms,
            outcome: reply.outcome,
        },
    )
    .context("write API response")?;
    output.write_all(b"\n").context("finish API response")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_fields_do_not_change_operation_identity() {
        let request = |context: &str, expected_server_id: Option<&str>| Request::DeleteWorld {
            routing: Routing {
                api_version: API_VERSION,
                request_id: Uuid::new_v4().to_string(),
                expected_server_id: expected_server_id.map(str::to_owned),
                context: context.to_owned(),
            },
            world_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        };
        let (_, first) = request_to_operation(request("old-alias", None)).unwrap();
        let (_, second) = request_to_operation(request(
            "new-alias",
            Some("22222222-2222-4222-8222-222222222222"),
        ))
        .unwrap();

        assert_eq!(operation_hash(&first), operation_hash(&second));
    }
}
