use anyhow::{bail, Context as _, Result};
use sha2::{Digest, Sha256};
use std::io::{Read as _, Write as _};
use uuid::Uuid;
use wt_control_protocol::{
    ApiRequest, CapacityResource, CreateWorld, ErrorCode, Operation, Outcome, Response, World,
    WorldStatus,
};

mod generated;
use generated::*;
#[cfg(test)]
mod tests;

const API_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 128 * 1024 * 1024;

impl Request {
    fn api_version(&self) -> u32 {
        match self {
            Self::ListContexts { api_version, .. }
            | Self::ExecWorld { api_version, .. }
            | Self::ListWorlds { api_version, .. }
            | Self::CreateWorld { api_version, .. }
            | Self::DeleteWorld { api_version, .. }
            | Self::StartCodex { api_version, .. }
            | Self::InspectCodex { api_version, .. }
            | Self::ResumeCodex { api_version, .. }
            | Self::SendCodexMessage { api_version, .. }
            | Self::SteerCodex { api_version, .. }
            | Self::InterruptCodex { api_version, .. }
            | Self::ReadWorldMail { api_version, .. } => *api_version,
        }
    }

    fn request_id(&self) -> &str {
        match self {
            Self::ListContexts { request_id, .. }
            | Self::ExecWorld { request_id, .. }
            | Self::ListWorlds { request_id, .. }
            | Self::CreateWorld { request_id, .. }
            | Self::DeleteWorld { request_id, .. }
            | Self::StartCodex { request_id, .. }
            | Self::InspectCodex { request_id, .. }
            | Self::ResumeCodex { request_id, .. }
            | Self::SendCodexMessage { request_id, .. }
            | Self::SteerCodex { request_id, .. }
            | Self::InterruptCodex { request_id, .. }
            | Self::ReadWorldMail { request_id, .. } => request_id,
        }
    }

    fn expected_server_id(&self) -> Option<&str> {
        match self {
            Self::ListContexts { .. } => None,
            Self::ListWorlds {
                expected_server_id, ..
            }
            | Self::ExecWorld {
                expected_server_id, ..
            }
            | Self::CreateWorld {
                expected_server_id, ..
            }
            | Self::DeleteWorld {
                expected_server_id, ..
            }
            | Self::StartCodex {
                expected_server_id, ..
            }
            | Self::InspectCodex {
                expected_server_id, ..
            }
            | Self::ResumeCodex {
                expected_server_id, ..
            }
            | Self::SendCodexMessage {
                expected_server_id, ..
            }
            | Self::SteerCodex {
                expected_server_id, ..
            }
            | Self::InterruptCodex {
                expected_server_id, ..
            }
            | Self::ReadWorldMail {
                expected_server_id, ..
            } => expected_server_id.as_deref(),
        }
    }
}

impl From<wt_control_protocol::CodexStatus> for ApiCodexStatus {
    fn from(status: wt_control_protocol::CodexStatus) -> Self {
        match status {
            wt_control_protocol::CodexStatus::Active => Self::Active,
            wt_control_protocol::CodexStatus::Idle => Self::Idle,
            wt_control_protocol::CodexStatus::Error => Self::Error,
        }
    }
}

impl From<wt_control_protocol::CodexMessageDelivery> for ApiCodexMessageDelivery {
    fn from(delivery: wt_control_protocol::CodexMessageDelivery) -> Self {
        match delivery {
            wt_control_protocol::CodexMessageDelivery::Steered => Self::Steered,
            wt_control_protocol::CodexMessageDelivery::Started => Self::Started,
            wt_control_protocol::CodexMessageDelivery::InterruptRequested => {
                Self::InterruptRequested
            }
        }
    }
}

impl From<World> for ApiWorld {
    fn from(world: World) -> Self {
        Self {
            world_id: world.world_id.to_string(),
            name: world.name.to_string(),
            status: world.status.into(),
            vcpus: world.vcpus,
            memory_mib: world.memory_mib,
            disk_gib: world.disk_gib,
            guest_ip: world.guest_ip,
            last_error: world.last_error,
            ssh: world.ssh.map(Into::into),
        }
    }
}

impl From<WorldStatus> for ApiWorldStatus {
    fn from(status: WorldStatus) -> Self {
        match status {
            WorldStatus::Provisioning => Self::Provisioning,
            WorldStatus::Running => Self::Running,
            WorldStatus::Stopped => Self::Stopped,
            WorldStatus::Destroying => Self::Destroying,
            WorldStatus::Error => Self::Error,
        }
    }
}

impl From<wt_control_protocol::SshAccess> for ApiSshAccess {
    fn from(access: wt_control_protocol::SshAccess) -> Self {
        Self {
            user: access.user,
            host: access.host,
            port: access.port,
            host_keys: access.host_keys,
        }
    }
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
    outcome: ReplyOutcome,
}

enum ReplyOutcome {
    Ok { result: ApiResult },
    Error { error: ApiError },
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
            );
        }
    };
    let expected_server_id = match request.expected_server_id().map(str::parse::<Uuid>) {
        Some(Ok(server_id)) => Some(server_id),
        Some(Err(_)) => {
            return write_local_error(
                Some(request_id.to_string()),
                "invalid_request",
                "invalid expected server ID".to_owned(),
            );
        }
        None => None,
    };
    let config = match wt_client::config::ClientConfig::load() {
        Ok(config) => config,
        Err(error) => {
            return write_local_error(
                Some(request_id.to_string()),
                "configuration_error",
                format!("{error:#}"),
            );
        }
    };
    if matches!(request, Request::ListContexts { .. }) {
        return finish(Reply {
            request_id: Some(request_id.to_string()),
            server_id: None,
            expires_at_unix_ms: None,
            outcome: ReplyOutcome::Ok {
                result: ApiResult::ListContexts {
                    contexts: config
                        .contexts
                        .iter()
                        .map(|context| context.name.to_string())
                        .collect(),
                },
            },
        });
    }
    let (context_name, operation) = match request_to_operation(request) {
        Ok(request) => request,
        Err(message) => {
            return write_local_error(Some(request_id.to_string()), "invalid_request", message);
        }
    };
    let request_hash = operation_hash(&operation);
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
        Request::ExecWorld {
            context,
            world_id,
            executable,
            args,
            stdin,
            ..
        } => Ok((
            context,
            Operation::ExecWorld {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                command: wt_control_protocol::ExecCommand {
                    executable,
                    args,
                    stdin,
                },
            },
        )),
        Request::ListContexts { .. } => Err("list_contexts is a client-local operation".to_owned()),
        Request::ListWorlds { context, .. } => Ok((context, Operation::ListWorlds)),
        Request::CreateWorld {
            context,
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
                context,
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
            context, world_id, ..
        } => {
            let world_id = world_id
                .parse()
                .map_err(|_| "invalid world ID".to_owned())?;
            Ok((context, Operation::DeleteWorld { world_id }))
        }
        Request::StartCodex {
            context,
            world_id,
            message,
            ..
        } => Ok((
            context,
            Operation::StartCodex {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                message,
            },
        )),
        Request::InspectCodex {
            context,
            world_id,
            thread_id,
            ..
        } => Ok((
            context,
            Operation::InspectCodex {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                thread_id,
            },
        )),
        Request::ResumeCodex {
            context,
            world_id,
            thread_id,
            ..
        } => Ok((
            context,
            Operation::ResumeCodex {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                thread_id,
            },
        )),
        Request::SendCodexMessage {
            context,
            world_id,
            thread_id,
            message,
            ..
        } => Ok((
            context,
            Operation::SendCodexMessage {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                thread_id,
                message,
            },
        )),
        Request::SteerCodex {
            context,
            world_id,
            thread_id,
            turn_id,
            message,
            ..
        } => Ok((
            context,
            Operation::SteerCodex {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                thread_id,
                turn_id,
                message,
            },
        )),
        Request::InterruptCodex {
            context,
            world_id,
            thread_id,
            turn_id,
            ..
        } => Ok((
            context,
            Operation::InterruptCodex {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                thread_id,
                turn_id,
            },
        )),
        Request::ReadWorldMail {
            context,
            world_id,
            after_message_id,
            limit,
            ..
        } => Ok((
            context,
            Operation::ListWorldMail {
                world_id: world_id
                    .parse()
                    .map_err(|_| "invalid world ID".to_owned())?,
                after_id: after_message_id,
                limit,
            },
        )),
    }
}

fn call(
    context: &wt_client::config::Context,
    request_id: Uuid,
    request_hash: String,
    expected_server_id: Option<Uuid>,
    operation: Operation,
) -> Reply {
    let api_request = if matches!(
        &operation,
        Operation::ListWorlds
            | Operation::ListWorldMail { .. }
            | Operation::InspectCodex { .. }
            | Operation::ExecWorld { .. }
    ) {
        ApiRequest {
            protocol_version: wt_control_protocol::PROTOCOL_VERSION,
            request_id: Some(request_id),
            request_hash: None,
            expected_server_id,
            operation,
        }
    } else {
        ApiRequest::with_request_id(operation, request_id, request_hash, expected_server_id)
    };
    let response =
        match wt_client::transport::call_response_with_progress(context, &api_request, |_| {}) {
            Ok(response) => response,
            Err(error) => {
                return Reply {
                    request_id: Some(request_id.to_string()),
                    server_id: None,
                    expires_at_unix_ms: None,
                    outcome: api_error("context_error", error.to_string(), true, None),
                };
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
                Response::WorldExecuted { output } => ReplyOutcome::Ok {
                    result: ApiResult::ExecWorld {
                        stdout: output.stdout,
                        stderr: output.stderr,
                        exit_status: i64::from(output.exit_status),
                    },
                },
                Response::Worlds { worlds, .. } => ReplyOutcome::Ok {
                    result: ApiResult::ListWorlds {
                        worlds: worlds.into_iter().map(Into::into).collect(),
                    },
                },
                Response::World { world } => ReplyOutcome::Ok {
                    result: ApiResult::CreateWorld {
                        world: (*world).into(),
                    },
                },
                Response::WorldDeleted { world_id } => ReplyOutcome::Ok {
                    result: ApiResult::DeleteWorld {
                        world_id: world_id.to_string(),
                    },
                },
                Response::CodexStarted {
                    thread_id,
                    turn_id,
                    pane_id,
                    window_name,
                } => ReplyOutcome::Ok {
                    result: ApiResult::StartCodex {
                        thread_id,
                        turn_id,
                        pane_id,
                        window_name,
                    },
                },
                Response::CodexInspection {
                    thread_id,
                    status,
                    active_turn_id,
                    pane_id,
                    window_name,
                    screen,
                    observed_at_unix_ms,
                } => ReplyOutcome::Ok {
                    result: ApiResult::InspectCodex {
                        thread_id,
                        status: status.into(),
                        active_turn_id,
                        pane_id,
                        window_name,
                        screen,
                        observed_at_unix_ms,
                    },
                },
                Response::CodexMessageSent {
                    thread_id,
                    turn_id,
                    delivery,
                } => ReplyOutcome::Ok {
                    result: ApiResult::SendCodexMessage {
                        thread_id,
                        turn_id,
                        delivery: delivery.into(),
                    },
                },
                Response::WorldMail {
                    messages,
                    high_water_id,
                } => ReplyOutcome::Ok {
                    result: ApiResult::ReadWorldMail {
                        messages: messages
                            .into_iter()
                            .map(|mail| ApiWorldMail {
                                message_id: mail.id,
                                world_id: mail.world_id.to_string(),
                                thread_id: mail.thread_id,
                                turn_id: mail.turn_id,
                                pane_id: mail.pane_id,
                                created_at_unix_ms: mail.created_at_unix_ms,
                                kind: match mail.kind {
                                    wt_control_protocol::MailKind::Message => ApiMailKind::Message,
                                    wt_control_protocol::MailKind::Completed => {
                                        ApiMailKind::Completed
                                    }
                                    wt_control_protocol::MailKind::Failed => ApiMailKind::Failed,
                                },
                                text: mail.message,
                            })
                            .collect(),
                        high_water_message_id: high_water_id,
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
                let details = error.capacity.map(|capacity| ApiCapacityDetails {
                    kind: "capacity".to_owned(),
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
    details: Option<ApiCapacityDetails>,
) -> ReplyOutcome {
    ReplyOutcome::Error {
        error: ApiError {
            code: code.to_owned(),
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
    let failed = matches!(reply.outcome, ReplyOutcome::Error { .. });
    write_response(reply)?;
    if failed {
        eprintln!("wt api: request failed");
        bail!("API request failed")
    }
    Ok(())
}

fn write_response(reply: Reply) -> Result<()> {
    let response = match reply.outcome {
        ReplyOutcome::Ok { result } => ApiResponse::Ok {
            api_version: API_VERSION,
            request_id: reply
                .request_id
                .context("successful API response is missing its request ID")?,
            server_id: reply.server_id,
            expires_at_unix_ms: reply.expires_at_unix_ms,
            result,
        },
        ReplyOutcome::Error { error } => ApiResponse::Error {
            api_version: API_VERSION,
            request_id: reply.request_id,
            server_id: reply.server_id,
            expires_at_unix_ms: reply.expires_at_unix_ms,
            error,
        },
    };
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, &response).context("write API response")?;
    output.write_all(b"\n").context("finish API response")?;
    Ok(())
}
