use crate::config::{ClientConfig, Context};
use crate::transport::{self, ContextError};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wt_control_protocol::{ApiRequest, Operation, ResourceCapacity, Response, World, WorldName};

#[derive(Clone, Debug)]
pub struct ContextWorld {
    pub context: String,
    pub world: World,
    pub agent_tool_report_count: u64,
    pub disk_usage_bytes: Option<u64>,
}

#[derive(Debug)]
pub struct WorldInventory {
    pub worlds: Vec<ContextWorld>,
    pub capacity: ResourceCapacity,
    pub capacity_by_context: BTreeMap<String, ResourceCapacity>,
    pub failures: Vec<ContextError>,
}

impl ContextWorld {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.context, self.world.name)
    }
}

pub fn format_resources(world: &World, disk_usage_bytes: Option<u64>) -> String {
    let memory = if world.memory_mib.is_multiple_of(1024) {
        format!("{}G", world.memory_mib / 1024)
    } else {
        format!("{}MiB", world.memory_mib)
    };
    let disk = disk_usage_bytes.map_or_else(
        || format!("{}G", world.disk_gib),
        |bytes| {
            let usage = format_disk_usage(bytes);
            if world.status == wt_control_protocol::WorldStatus::Stopped {
                format!("{usage} disk")
            } else {
                format!("{usage}/{}G disk", world.disk_gib)
            }
        },
    );
    format!("{} CPU · {memory} · {disk}", world.vcpus)
}

pub fn format_capacity(capacity: ResourceCapacity) -> Option<String> {
    if capacity.total == Default::default() {
        return None;
    }
    Some(format!(
        "CPU {}/{} · RAM {}/{} · Disk {}G/{}G",
        capacity.reserved.vcpus,
        capacity.total.vcpus,
        format_memory(capacity.reserved.memory_mib),
        format_memory(capacity.total.memory_mib),
        capacity.reserved.disk_gib,
        capacity.total.disk_gib,
    ))
}

fn format_memory(memory_mib: u64) -> String {
    if memory_mib.is_multiple_of(1024) {
        format!("{}G", memory_mib / 1024)
    } else {
        format!("{memory_mib}MiB")
    }
}

pub fn format_detail(item: &ContextWorld) -> String {
    let world = &item.world;
    let target = format!("{}.{}", item.context, world.name);
    let detail = match world.status {
        wt_control_protocol::WorldStatus::Stopped => format!(
            "{}; run `wt start {target}` or `wt rm {target}`",
            world.last_error.as_deref().unwrap_or("guest stopped")
        ),
        wt_control_protocol::WorldStatus::Error => format!(
            "{}; run `wt rm {target}`",
            world.last_error.as_deref().unwrap_or("world failed")
        ),
        _ => world.last_error.as_deref().unwrap_or("-").to_owned(),
    };
    if item.agent_tool_report_count == 0 {
        return detail;
    }
    let reports = format!(
        "{} wt-tools report{}; run `wt reports`",
        item.agent_tool_report_count,
        if item.agent_tool_report_count == 1 {
            ""
        } else {
            "s"
        }
    );
    if detail == "-" {
        reports
    } else {
        format!("{detail}; {reports}")
    }
}

fn format_disk_usage(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes == 0 {
        return "0B".to_owned();
    }
    if bytes >= GIB {
        let tenths = (u128::from(bytes) * 10).div_ceil(u128::from(GIB));
        if tenths.is_multiple_of(10) {
            format!("{}G", tenths / 10)
        } else {
            format!("{}.{}G", tenths / 10, tenths % 10)
        }
    } else if bytes >= MIB {
        format!("{}M", bytes.div_ceil(MIB))
    } else {
        format!("{}K", bytes.div_ceil(KIB))
    }
}

pub fn list_all(config: &ClientConfig) -> WorldInventory {
    list_all_inner(config, None)
}

pub fn list_all_with_timeout(
    config: &ClientConfig,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> WorldInventory {
    list_all_inner(config, Some((timeout, cancelled)))
}

fn list_all_inner(
    config: &ClientConfig,
    timeout: Option<(Duration, &AtomicBool)>,
) -> WorldInventory {
    let mut all = Vec::new();
    let mut capacity = ResourceCapacity::default();
    let mut capacity_by_context = BTreeMap::new();
    let mut failures = Vec::new();
    for context in &config.contexts {
        if timeout.is_some_and(|(_, cancelled)| cancelled.load(Ordering::Relaxed)) {
            break;
        }
        let request = ApiRequest::new(Operation::ListWorlds);
        let response = match timeout.map_or_else(
            || transport::call(context, &request),
            |(timeout, cancelled)| {
                transport::call_with_timeout_until(context, &request, timeout, cancelled)
            },
        ) {
            Ok(response) => response,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let Response::Worlds {
            worlds,
            capacity: context_capacity,
            disk_usage_bytes,
            agent_tool_report_counts,
        } = response
        else {
            failures.push(transport::wrong_response(context, "list"));
            continue;
        };
        capacity = capacity.saturating_add(context_capacity);
        capacity_by_context.insert(context.name.clone(), context_capacity);
        all.extend(worlds.into_iter().map(|world| {
            let agent_tool_report_count = agent_tool_report_counts
                .get(&world.world_id)
                .copied()
                .unwrap_or_default();
            let disk_usage_bytes = disk_usage_bytes.get(&world.world_id).copied();
            ContextWorld {
                context: context.name.clone(),
                world,
                agent_tool_report_count,
                disk_usage_bytes,
            }
        }));
    }
    group_by_context(&mut all);
    WorldInventory {
        worlds: all,
        capacity,
        capacity_by_context,
        failures,
    }
}

fn group_by_context(worlds: &mut [ContextWorld]) {
    // Stable sorting groups contexts without changing the server's creation order.
    worlds.sort_by(|left, right| left.context.cmp(&right.context));
}

pub fn parse_target<'a>(
    config: &'a ClientConfig,
    target: &str,
) -> Result<(Option<&'a Context>, WorldName)> {
    if let Some((context_name, world_name)) = target.split_once('.') {
        if world_name.contains('.') {
            bail!("invalid qualified world name: {target}");
        }
        let context = config
            .context(context_name)
            .ok_or_else(|| anyhow::anyhow!("unknown context: {context_name}"))?;
        return Ok((Some(context), WorldName::parse(world_name)?));
    }
    Ok((None, WorldName::parse(target)?))
}

pub fn resolve<'a>(inventory: &'a [ContextWorld], target: &str) -> Result<&'a ContextWorld> {
    if let Some((context, name)) = target.split_once('.') {
        return inventory
            .iter()
            .find(|item| item.context == context && item.world.name.as_str() == name)
            .ok_or_else(|| anyhow::anyhow!("world not found: {target}"));
    }
    let matches = inventory
        .iter()
        .filter(|item| item.world.name.as_str() == target)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => bail!("world not found: {target}"),
        [item] => Ok(item),
        _ => {
            let names = matches
                .iter()
                .map(|item| item.qualified_name())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("world name is ambiguous: {target}; use one of: {names}")
        }
    }
}

pub fn name_counts(inventory: &[ContextWorld]) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for item in inventory {
        *counts.entry(item.world.name.as_str()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_control_protocol::WorldStatus;

    fn item(context: &str, name: &str) -> ContextWorld {
        ContextWorld {
            context: context.into(),
            agent_tool_report_count: 0,
            disk_usage_bytes: None,
            world: World {
                world_id: Uuid::new_v4().into(),
                name: WorldName::parse(name).unwrap(),
                owner: "tester".into(),
                status: WorldStatus::Running,
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
                guest_ip: None,
                last_error: None,
                ssh: None,
            },
        }
    }

    #[test]
    fn resolves_unique_short_and_qualified_names() {
        let inventory = vec![item("local", "one"), item("lab", "two")];
        assert_eq!(resolve(&inventory, "one").unwrap().context, "local");
        assert_eq!(resolve(&inventory, "lab.two").unwrap().context, "lab");
    }

    #[test]
    fn ambiguous_short_name_lists_fqns() {
        let inventory = vec![item("local", "same"), item("lab", "same")];
        let error = resolve(&inventory, "same").unwrap_err().to_string();
        insta::assert_snapshot!(error, @"world name is ambiguous: same; use one of: local.same, lab.same");
    }

    #[test]
    fn grouping_contexts_preserves_server_world_order() {
        let mut inventory = vec![
            item("local", "w6"),
            item("lab", "other"),
            item("local", "w10"),
        ];

        group_by_context(&mut inventory);

        let names = inventory
            .iter()
            .map(ContextWorld::qualified_name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["lab.other", "local.w6", "local.w10"]);
    }

    #[test]
    fn formats_reserved_and_total_capacity() {
        let capacity = ResourceCapacity {
            reserved: wt_control_protocol::Resources {
                vcpus: 6,
                memory_mib: 10_240,
                disk_gib: 68,
            },
            total: wt_control_protocol::Resources {
                vcpus: 16,
                memory_mib: 32_768,
                disk_gib: 256,
            },
        };

        insta::assert_snapshot!(format_capacity(capacity).unwrap(), @"CPU 6/16 · RAM 10G/32G · Disk 68G/256G");
    }
}
