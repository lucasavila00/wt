use serde::Serialize;
use wt_control_protocol::{World, WorldStatus};

#[derive(Debug, Serialize)]
pub(super) struct ApiWorld {
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
