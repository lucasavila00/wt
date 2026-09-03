use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    CreateWorld {
        api_version: u32,
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
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
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
        context: String,
        world_id: String,
    },
    StartCodex {
        api_version: u32,
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
        context: String,
        world_id: String,
        message: String,
    },
    InspectCodex {
        api_version: u32,
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
        context: String,
        world_id: String,
        thread_id: String,
    },
    SendCodexMessage {
        api_version: u32,
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
        context: String,
        world_id: String,
        thread_id: String,
        message: String,
    },
    ReadWorldMail {
        api_version: u32,
        request_id: String,
        #[serde(default)]
        expected_server_id: Option<String>,
        context: String,
        world_id: String,
        after_message_id: u64,
        limit: u32,
    },
}

impl Request {
    pub(super) fn api_version(&self) -> u32 {
        match self {
            Self::CreateWorld { api_version, .. }
            | Self::DeleteWorld { api_version, .. }
            | Self::StartCodex { api_version, .. }
            | Self::InspectCodex { api_version, .. }
            | Self::SendCodexMessage { api_version, .. }
            | Self::ReadWorldMail { api_version, .. } => *api_version,
        }
    }

    pub(super) fn request_id(&self) -> &str {
        match self {
            Self::CreateWorld { request_id, .. }
            | Self::DeleteWorld { request_id, .. }
            | Self::StartCodex { request_id, .. }
            | Self::InspectCodex { request_id, .. }
            | Self::SendCodexMessage { request_id, .. }
            | Self::ReadWorldMail { request_id, .. } => request_id,
        }
    }

    pub(super) fn expected_server_id(&self) -> Option<&str> {
        match self {
            Self::CreateWorld {
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
            | Self::SendCodexMessage {
                expected_server_id, ..
            }
            | Self::ReadWorldMail {
                expected_server_id, ..
            } => expected_server_id.as_deref(),
        }
    }
}
