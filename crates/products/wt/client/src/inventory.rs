use crate::config::{ClientConfig, Context};
use crate::transport::{self, ContextError};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wt_control_protocol::{ApiRequest, Instance, InstanceName, Operation, Response};

#[derive(Clone, Debug)]
pub struct ContextInstance {
    pub context: String,
    pub instance: Instance,
    pub agent_tool_report_count: u64,
    pub disk_usage_bytes: Option<u64>,
}

#[derive(Debug)]
pub struct InventoryReport {
    pub instances: Vec<ContextInstance>,
    pub failures: Vec<ContextError>,
}

impl ContextInstance {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.context, self.instance.name)
    }
}

pub fn format_resources(instance: &Instance, disk_usage_bytes: Option<u64>) -> String {
    let memory = if instance.memory_mib.is_multiple_of(1024) {
        format!("{}G", instance.memory_mib / 1024)
    } else {
        format!("{}MiB", instance.memory_mib)
    };
    let disk = disk_usage_bytes.map_or_else(
        || format!("{}G", instance.disk_gib),
        |bytes| {
            let usage = format_disk_usage(bytes);
            if instance.status == wt_control_protocol::InstanceStatus::Stopped {
                format!("{usage} disk")
            } else {
                format!("{usage}/{}G disk", instance.disk_gib)
            }
        },
    );
    format!("{} CPU · {memory} · {disk}", instance.vcpus)
}

pub fn format_detail(item: &ContextInstance) -> String {
    let instance = &item.instance;
    let target = format!("{}.{}", item.context, instance.name);
    let detail = match instance.status {
        wt_control_protocol::InstanceStatus::Stopped => format!(
            "{}; run `wt start {target}` or `wt rm {target}`",
            instance.last_error.as_deref().unwrap_or("guest stopped")
        ),
        wt_control_protocol::InstanceStatus::Error => format!(
            "{}; run `wt rm {target}`",
            instance.last_error.as_deref().unwrap_or("world failed")
        ),
        _ => instance.last_error.as_deref().unwrap_or("-").to_owned(),
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

pub fn list_all(config: &ClientConfig) -> InventoryReport {
    list_all_inner(config, None)
}

pub fn list_all_with_timeout(
    config: &ClientConfig,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> InventoryReport {
    list_all_inner(config, Some((timeout, cancelled)))
}

fn list_all_inner(
    config: &ClientConfig,
    timeout: Option<(Duration, &AtomicBool)>,
) -> InventoryReport {
    let mut all = Vec::new();
    let mut failures = Vec::new();
    for context in &config.contexts {
        if timeout.is_some_and(|(_, cancelled)| cancelled.load(Ordering::Relaxed)) {
            break;
        }
        let request = ApiRequest::new(Operation::List);
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
        let Response::Instances {
            instances,
            disk_usage_bytes,
            agent_tool_report_counts,
        } = response
        else {
            failures.push(transport::wrong_response(context, "list"));
            continue;
        };
        all.extend(instances.into_iter().map(|instance| {
            let agent_tool_report_count = agent_tool_report_counts
                .get(&instance.id)
                .copied()
                .unwrap_or_default();
            let disk_usage_bytes = disk_usage_bytes.get(&instance.id).copied();
            ContextInstance {
                context: context.name.clone(),
                instance,
                agent_tool_report_count,
                disk_usage_bytes,
            }
        }));
    }
    group_by_context(&mut all);
    InventoryReport {
        instances: all,
        failures,
    }
}

fn group_by_context(instances: &mut [ContextInstance]) {
    // Stable sorting groups contexts without changing the server's creation order.
    instances.sort_by(|left, right| left.context.cmp(&right.context));
}

pub fn parse_target<'a>(
    config: &'a ClientConfig,
    target: &str,
) -> Result<(Option<&'a Context>, InstanceName)> {
    if let Some((context_name, world_name)) = target.split_once('.') {
        if world_name.contains('.') {
            bail!("invalid qualified world name: {target}");
        }
        let context = config
            .context(context_name)
            .ok_or_else(|| anyhow::anyhow!("unknown context: {context_name}"))?;
        return Ok((Some(context), InstanceName::parse(world_name)?));
    }
    Ok((None, InstanceName::parse(target)?))
}

pub fn resolve<'a>(inventory: &'a [ContextInstance], target: &str) -> Result<&'a ContextInstance> {
    if let Some((context, name)) = target.split_once('.') {
        return inventory
            .iter()
            .find(|item| item.context == context && item.instance.name.as_str() == name)
            .ok_or_else(|| anyhow::anyhow!("world not found: {target}"));
    }
    let matches = inventory
        .iter()
        .filter(|item| item.instance.name.as_str() == target)
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

pub fn name_counts(inventory: &[ContextInstance]) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for item in inventory {
        *counts.entry(item.instance.name.as_str()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_control_protocol::InstanceStatus;

    fn item(context: &str, name: &str) -> ContextInstance {
        ContextInstance {
            context: context.into(),
            agent_tool_report_count: 0,
            disk_usage_bytes: None,
            instance: Instance {
                id: Uuid::new_v4(),
                name: InstanceName::parse(name).unwrap(),
                owner: "tester".into(),
                status: InstanceStatus::Running,
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
            .map(ContextInstance::qualified_name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["lab.other", "local.w6", "local.w10"]);
    }
}
