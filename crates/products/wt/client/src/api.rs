use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read as _, Write as _};
use wt_control_protocol::{
    ApiRequest, CapacityResource, CreateWorld, ErrorCode, Operation, Outcome, Response, World,
    WorldStatus,
};

const API_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    CreateWorld {
        api_version: u32,
        context: String,
        name: String,
        vcpus: u32,
        memory_mib: u64,
        disk_gib: u64,
        git_user_name: String,
        git_user_email: String,
    },
    DeleteWorld {
        api_version: u32,
        context: String,
        world_id: String,
    },
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    api_version: u32,
    #[serde(flatten)]
    outcome: ApiOutcome,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum ApiOutcome {
    Ok { response: ApiSuccess },
    Error { error: ApiError },
}

#[derive(Debug, Serialize)]
#[serde(tag = "response", rename_all = "snake_case")]
enum ApiSuccess {
    World { world: ApiWorld },
    WorldDeleted { world_id: String },
}

#[derive(Debug, Serialize)]
struct ApiWorld {
    world_id: String,
    name: String,
    status: ApiWorldStatus,
    vcpus: u32,
    memory_mib: u64,
    disk_gib: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    guest_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh: Option<ApiSshAccess>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiWorldStatus {
    Provisioning,
    Running,
    Stopped,
    Destroying,
    Error,
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

#[derive(Debug, Serialize)]
struct ApiSshAccess {
    user: String,
    host: String,
    port: u16,
    host_keys: Vec<String>,
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

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    /// Diagnostic text, not a machine-stable value.
    message: String,
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

pub fn run() -> Result<()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut input)
        .context("read API request")?;
    if input.len() as u64 > MAX_REQUEST_BYTES {
        return write_error(
            "invalid_request",
            format!("API request exceeds {MAX_REQUEST_BYTES} bytes"),
        );
    }
    let request = match serde_json::from_slice::<Request>(&input) {
        Ok(request) => request,
        Err(error) => return write_parse_error(error),
    };
    let (api_version, context_name, operation) = match request_to_operation(request) {
        Ok(request) => request,
        Err((code, message)) => return write_error(code, message),
    };
    if api_version != API_VERSION {
        return write_error(
            "unsupported_api_version",
            format!("unsupported API version {api_version}; expected {API_VERSION}"),
        );
    }
    let config = match wt_client::config::ClientConfig::load() {
        Ok(config) => config,
        Err(error) => return write_error("configuration_error", format!("{error:#}")),
    };
    let Some(context) = config.context(&context_name) else {
        return write_error("unknown_context", "unknown context".to_owned());
    };
    finish(wt_client_response(context, operation))
}

fn request_to_operation(
    request: Request,
) -> std::result::Result<(u32, String, Operation), (&'static str, String)> {
    match request {
        Request::CreateWorld {
            api_version,
            context,
            name,
            vcpus,
            memory_mib,
            disk_gib,
            git_user_name,
            git_user_email,
        } => {
            let name = wt_control_protocol::WorldName::parse(name)
                .map_err(|_| ("invalid_request", "invalid world name".to_owned()))?;
            if git_user_name.is_empty() || git_user_email.is_empty() {
                return Err((
                    "invalid_request",
                    "git_user_name and git_user_email must not be empty".to_owned(),
                ));
            }
            Ok((
                api_version,
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
            api_version,
            context,
            world_id,
        } => {
            let world_id = world_id
                .parse()
                .map_err(|_| ("invalid_request", "invalid world ID".to_owned()))?;
            Ok((api_version, context, Operation::DeleteWorld { world_id }))
        }
    }
}

fn wt_client_response(context: &wt_client::config::Context, operation: Operation) -> ApiOutcome {
    let deleted_world_id = match &operation {
        Operation::DeleteWorld { world_id } => Some(world_id.to_string()),
        _ => None,
    };
    match wt_client::transport::call_outcome_with_progress(
        context,
        &ApiRequest::new(operation),
        |_| {},
    ) {
        Ok(Outcome::Ok { response }) => match *response {
            Response::World { world } => ApiOutcome::Ok {
                response: ApiSuccess::World {
                    world: (*world).into(),
                },
            },
            Response::WorldDeleted { world_id } => ApiOutcome::Ok {
                response: ApiSuccess::WorldDeleted {
                    world_id: world_id.to_string(),
                },
            },
            _ => error("internal_error", "server returned an unexpected response"),
        },
        Ok(Outcome::Error {
            error: server_error,
        }) if server_error.code == ErrorCode::NotFound && deleted_world_id.is_some() => {
            ApiOutcome::Ok {
                response: ApiSuccess::WorldDeleted {
                    world_id: deleted_world_id.expect("checked above"),
                },
            }
        }
        Ok(Outcome::Error {
            error: server_error,
        }) => error_with_details(
            error_code(server_error.code),
            server_error.message,
            server_error
                .capacity
                .map(|capacity| ApiErrorDetails::Capacity {
                    resource: capacity.resource.into(),
                    total: capacity.total,
                    reserved: capacity.reserved,
                    requested: capacity.requested,
                }),
        ),
        Err(context_error) => error("context_error", context_error.to_string()),
    }
}

fn error(code: &'static str, message: impl Into<String>) -> ApiOutcome {
    error_with_details(code, message, None)
}

fn error_with_details(
    code: &'static str,
    message: impl Into<String>,
    details: Option<ApiErrorDetails>,
) -> ApiOutcome {
    ApiOutcome::Error {
        error: ApiError {
            code,
            message: message.into(),
            details,
        },
    }
}

fn error_code(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidRequest => "invalid_request",
        ErrorCode::UnsupportedProtocol => "unsupported_protocol",
        ErrorCode::Conflict => "conflict",
        ErrorCode::NotFound => "not_found",
        ErrorCode::Capacity => "capacity",
        ErrorCode::Backend => "backend_error",
        ErrorCode::Internal => "internal_error",
    }
}

fn write_error(code: &'static str, message: String) -> Result<()> {
    eprintln!("wt api: {message}");
    write_response(error(code, message))?;
    bail!("API request failed")
}

fn write_parse_error(parse_error: serde_json::Error) -> Result<()> {
    eprintln!("wt api: invalid JSON request: {parse_error}");
    write_response(error("invalid_request", "invalid JSON request"))?;
    bail!("API request failed")
}

fn finish(outcome: ApiOutcome) -> Result<()> {
    let failed = matches!(outcome, ApiOutcome::Error { .. });
    write_response(outcome)?;
    if failed {
        eprintln!("wt api: request failed");
        bail!("API request failed")
    }
    Ok(())
}

fn write_response(outcome: ApiOutcome) -> Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(
        &mut output,
        &ApiResponse {
            api_version: API_VERSION,
            outcome,
        },
    )
    .context("write API response")?;
    output.write_all(b"\n").context("finish API response")?;
    Ok(())
}
