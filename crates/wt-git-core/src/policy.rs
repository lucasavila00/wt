use crate::packet::push_commands;
use anyhow::{bail, Result};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WritePolicy {
    prefix: String,
    branches: BTreeSet<String>,
}

impl WritePolicy {
    pub fn new(
        prefix: impl Into<String>,
        branches: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let prefix = prefix.into();
        let Some(name) = prefix.strip_prefix("refs/heads/") else {
            bail!("write prefix must start with refs/heads/");
        };
        if !prefix.ends_with('/') || !valid_branch_name(name.trim_end_matches('/')) {
            bail!("invalid write prefix");
        }
        let branches = branches.into_iter().collect::<BTreeSet<_>>();
        if branches.iter().any(|branch| {
            branch
                .strip_prefix("refs/heads/")
                .is_none_or(|name| !valid_branch_name(name))
        }) {
            bail!("allowed branches must be fully qualified branch refs");
        }
        Ok(Self { prefix, branches })
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn branches(&self) -> &BTreeSet<String> {
        &self.branches
    }

    pub fn permits(&self, reference: &str) -> bool {
        self.branches.contains(reference)
            || (reference.starts_with(&self.prefix) && reference.len() > self.prefix.len())
    }
}

fn valid_branch_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(".lock")
                && part.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PushViolation {
    NonBranch { reference: String },
    Unauthorized { reference: String },
}

impl std::fmt::Display for PushViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonBranch { .. } => {
                formatter.write_str("tags and non-branch refs cannot be pushed")
            }
            Self::Unauthorized { reference } => {
                write!(formatter, "branch ref `{reference}` is outside the write policy")
            }
        }
    }
}

pub fn validate_push(section: &[u8], policy: &WritePolicy) -> Result<()> {
    if let Some(violation) = push_violation(section, policy)? {
        bail!(violation);
    }
    Ok(())
}

pub(crate) fn push_violation(
    section: &[u8],
    policy: &WritePolicy,
) -> Result<Option<PushViolation>> {
    for (_, reference) in push_commands(section)? {
        if !reference.starts_with("refs/heads/") {
            return Ok(Some(PushViolation::NonBranch { reference }));
        }
        if !policy.permits(&reference) {
            return Ok(Some(PushViolation::Unauthorized { reference }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::write_packet;

    fn commands(references: &[&str]) -> Vec<u8> {
        let mut section = Vec::new();
        for (index, reference) in references.iter().enumerate() {
            let capabilities = if index == 0 { "\0report-status" } else { "" };
            let payload = format!(
                "{} {} {reference}{capabilities}\n",
                "0".repeat(40),
                "a".repeat(40)
            );
            write_packet(&mut section, payload.as_bytes()).unwrap();
        }
        section.extend_from_slice(b"0000");
        section
    }

    #[test]
    fn permits_the_prefix_and_exact_branches() {
        let policy = WritePolicy::new(
            "refs/heads/agents/task-42/",
            ["refs/heads/main".to_owned()],
        )
        .unwrap();
        assert!(policy.permits("refs/heads/agents/task-42/fix"));
        assert!(policy.permits("refs/heads/main"));
        assert!(!policy.permits("refs/heads/agents/task-420/fix"));
        assert!(!policy.permits("refs/tags/main"));
    }

    #[test]
    fn validates_every_ref_in_a_push_transaction() {
        let policy = WritePolicy::new("refs/heads/wt/", []).unwrap();
        assert!(validate_push(&commands(&["refs/heads/wt/fix"]), &policy).is_ok());
        assert!(validate_push(
            &commands(&["refs/heads/wt/fix", "refs/heads/main"]),
            &policy
        )
        .is_err());
        assert!(validate_push(&commands(&["refs/tags/v1"]), &policy).is_err());
    }

    #[test]
    fn rejects_ambiguous_policy_values() {
        assert!(WritePolicy::new("wt/", []).is_err());
        assert!(WritePolicy::new("refs/heads/wt", []).is_err());
        assert!(WritePolicy::new("refs/heads/wt/", ["main".to_owned()]).is_err());
    }
}
