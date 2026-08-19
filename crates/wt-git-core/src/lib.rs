//! Shared Git transport and write authorization.

mod packet;
mod policy;
mod transport;

use serde::{Deserialize, Serialize};

pub use packet::{push_uses_sideband, successful_push_updates, write_packet};
pub use policy::{validate_push, PushViolation, WritePolicy};
pub use transport::{repository_refs, serve_git, DuplexStream, GitTarget};

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
