use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ControlRequest {
    Reserve {
        world_id: String,
        source: String,
        base: String,
        prefix: String,
    },
    Revoke {
        grant_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant: Option<Grant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ControlResponse {
    pub fn ok(grant: Option<Grant>) -> Self {
        Self {
            ok: true,
            grant,
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            grant: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Grant {
    pub id: String,
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientRequest {
    pub protocol_version: u32,
    #[serde(flatten)]
    pub operation: ClientOperation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ClientOperation {
    Git {
        service: GitService,
        source: String,
    },
    Cli {
        args: Vec<String>,
        branch: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportRequest {
    pub protocol_version: u32,
    pub token: String,
    #[serde(flatten)]
    pub operation: ClientOperation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TransportResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            message: None,
        }
    }

    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            error: None,
            message: Some(message.into()),
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
            message: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GitService {
    #[serde(rename = "git-upload-pack")]
    UploadPack,
    #[serde(rename = "git-receive-pack")]
    ReceivePack,
}

impl GitService {
    pub fn command(self) -> &'static str {
        match self {
            Self::UploadPack => "git-upload-pack",
            Self::ReceivePack => "git-receive-pack",
        }
    }
}

impl TryFrom<&str> for GitService {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "git-upload-pack" => Ok(Self::UploadPack),
            "git-receive-pack" => Ok(Self::ReceivePack),
            _ => Err("unsupported Git service"),
        }
    }
}
