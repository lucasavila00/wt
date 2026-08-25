use crate::WorldName;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateWorld {
    pub name: WorldName,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_name: String,
    #[serde(deserialize_with = "deserialize_nonempty_string")]
    pub git_user_email: String,
}

pub fn validate_create_world_resources(request: &CreateWorld) -> Result<(), &'static str> {
    if request.vcpus == 0 || request.memory_mib == 0 || request.disk_gib == 0 {
        return Err("CPU, memory, and disk values must be greater than zero");
    }
    Ok(())
}

fn deserialize_nonempty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("value must not be empty"));
    }
    Ok(value)
}
