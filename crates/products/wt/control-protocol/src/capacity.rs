use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    pub resource: CapacityResource,
    pub total: u64,
    pub reserved: u64,
    pub requested: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityResource {
    Cpu,
    Memory,
    Disk,
}

impl fmt::Display for CapacityResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cpu => "CPU",
            Self::Memory => "memory",
            Self::Disk => "disk",
        })
    }
}
