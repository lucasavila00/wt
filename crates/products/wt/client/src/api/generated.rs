// Generated from clients/elixir/wt_api/wt_api.schema.json. Do not edit.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub(super) enum Request {
    #[serde(rename = "create_world")]
    CreateWorld {
        api_version: u32,
        context: String,
        disk_gib: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        git_user_email: String,
        git_user_name: String,
        memory_mib: u64,
        name: String,
        request_id: String,
        vcpus: u32,
    },
    #[serde(rename = "delete_world")]
    DeleteWorld {
        api_version: u32,
        context: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        request_id: String,
        world_id: String,
    },
    #[serde(rename = "inspect_codex")]
    InspectCodex {
        api_version: u32,
        context: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        request_id: String,
        thread_id: String,
        world_id: String,
    },
    #[serde(rename = "read_world_mail")]
    ReadWorldMail {
        after_message_id: u64,
        api_version: u32,
        context: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        limit: u32,
        request_id: String,
        world_id: String,
    },
    #[serde(rename = "send_codex_message")]
    SendCodexMessage {
        api_version: u32,
        context: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        message: String,
        request_id: String,
        thread_id: String,
        world_id: String,
    },
    #[serde(rename = "start_codex")]
    StartCodex {
        api_version: u32,
        context: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_server_id: Option<String>,
        message: String,
        request_id: String,
        world_id: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome")]
pub(super) enum ApiResponse {
    #[serde(rename = "error")]
    Error {
        api_version: u32,
        error: ApiError,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_unix_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_id: Option<String>,
    },
    #[serde(rename = "ok")]
    Ok {
        api_version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_unix_ms: Option<i64>,
        request_id: String,
        result: ApiResult,
        server_id: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum ApiResult {
    CreateWorld {
        world: ApiWorld,
    },
    DeleteWorld {
        world_id: String,
    },
    InspectCodex {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active_turn_id: Option<String>,
        observed_at_unix_ms: i64,
        pane_id: String,
        screen: String,
        status: ApiCodexStatus,
        thread_id: String,
        window_name: String,
    },
    ReadWorldMail {
        high_water_message_id: u64,
        messages: Vec<ApiWorldMail>,
    },
    SendCodexMessage {
        delivery: ApiCodexMessageDelivery,
        thread_id: String,
        turn_id: String,
    },
    StartCodex {
        pane_id: String,
        thread_id: String,
        turn_id: String,
        window_name: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ApiWorldStatus {
    Destroying,
    Error,
    Provisioning,
    Running,
    Stopped,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ApiMailKind {
    Completed,
    Failed,
    Message,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ApiCodexStatus {
    Active,
    Error,
    Idle,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ApiCodexMessageDelivery {
    Started,
    Steered,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ApiCapacityResource {
    Cpu,
    Disk,
    Memory,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiSshAccess {
    pub(super) host: String,
    pub(super) host_keys: Vec<String>,
    pub(super) port: u16,
    pub(super) user: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiWorld {
    pub(super) disk_gib: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) guest_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_error: Option<String>,
    pub(super) memory_mib: u64,
    pub(super) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) ssh: Option<ApiSshAccess>,
    pub(super) status: ApiWorldStatus,
    pub(super) vcpus: u32,
    pub(super) world_id: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiWorldMail {
    pub(super) created_at_unix_ms: i64,
    pub(super) kind: ApiMailKind,
    pub(super) message_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) pane_id: Option<String>,
    pub(super) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) turn_id: Option<String>,
    pub(super) world_id: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiCapacityDetails {
    pub(super) kind: String,
    pub(super) requested: u64,
    pub(super) reserved: u64,
    pub(super) resource: ApiCapacityResource,
    pub(super) total: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiError {
    pub(super) code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) details: Option<ApiCapacityDetails>,
    pub(super) message: String,
    pub(super) retryable: bool,
}
