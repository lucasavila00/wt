use crate::config::{ClientConfig, Context};
use crate::transport;
use anyhow::{bail, Result};
use std::collections::HashMap;
use wt_api::{ApiRequest, Instance, InstanceName, Operation, Response};

#[derive(Clone, Debug)]
pub struct ContextInstance {
    pub context: String,
    pub instance: Instance,
}

impl ContextInstance {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.context, self.instance.name)
    }
}

pub fn list_all(config: &ClientConfig) -> Result<Vec<ContextInstance>> {
    let mut all = Vec::new();
    for context in &config.contexts {
        let response = transport::call(context, &ApiRequest::new(Operation::List))
            .map_err(|error| anyhow::anyhow!("context {}: {error:#}", context.name))?;
        let Response::Instances { instances } = response else {
            bail!(
                "context {} returned the wrong response to list",
                context.name
            );
        };
        all.extend(instances.into_iter().map(|instance| ContextInstance {
            context: context.name.clone(),
            instance,
        }));
    }
    all.sort_by(|left, right| {
        (&left.context, &left.instance.name).cmp(&(&right.context, &right.instance.name))
    });
    Ok(all)
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
    use wt_api::InstanceStatus;

    fn item(context: &str, name: &str) -> ContextInstance {
        ContextInstance {
            context: context.into(),
            instance: Instance {
                id: Uuid::new_v4(),
                name: InstanceName::parse(name).unwrap(),
                owner: "tester".into(),
                status: InstanceStatus::Running,
                source: "git@example.test:repo.git".into(),
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
        assert!(error.contains("local.same"));
        assert!(error.contains("lab.same"));
    }
}
