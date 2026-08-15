use crate::config::{ClientConfig, Context};
use anyhow::{bail, Context as _, Result};
use ssh_key::PublicKey;
use std::collections::BTreeSet;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Command;
use wt_api::{ClientEffect, ClientEffectOutput};

pub fn execute(
    config: &ClientConfig,
    context: &Context,
    effect: ClientEffect,
) -> Result<ClientEffectOutput> {
    match effect {
        ClientEffect::ReadGitIdentity => {
            let name = read_global_git_config("user.name")?;
            let email = read_global_git_config("user.email")?;
            Ok(ClientEffectOutput::GitIdentity { name, email })
        }
        ClientEffect::ReadSshPublicKeys => Ok(ClientEffectOutput::SshPublicKeys {
            keys: discover_public_keys()?,
        }),
        ClientEffect::ReplaceSshInventory { instances } => {
            crate::ssh::sync_context(config, context, &instances)?;
            Ok(ClientEffectOutput::None)
        }
        ClientEffect::LaunchCode { target } => {
            validate_target(context, &target)?;
            crate::code::launch(&target)?;
            Ok(ClientEffectOutput::None)
        }
        ClientEffect::ExecSsh { target } => {
            validate_target(context, &target)?;
            Err(Command::new("ssh").args(["--", &target]).exec())
                .with_context(|| format!("exec ssh {target}"))
        }
    }
}

fn validate_target(context: &Context, target: &str) -> Result<()> {
    let Some(name) = target.strip_prefix(&format!("{}.", context.name)) else {
        bail!("server requested a target outside context {}", context.name)
    };
    let name = name
        .strip_suffix("-host")
        .or_else(|| name.strip_suffix("-vs"))
        .unwrap_or(name);
    wt_api::InstanceName::parse(name.to_owned())?;
    Ok(())
}

fn read_global_git_config(key: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["config", "--global", "--null", "--get", key])
        .output()
        .with_context(|| format!("read global Git {key}"))?;
    if !output.status.success() {
        bail!("global Git {key} is required; configure it with `git config --global {key} VALUE`")
    }
    let value = output.stdout.strip_suffix(b"\0").unwrap_or(&output.stdout);
    let value = std::str::from_utf8(value).with_context(|| format!("decode global Git {key}"))?;
    if value.is_empty() {
        bail!("global Git {key} is required; configure it with `git config --global {key} VALUE`")
    }
    Ok(value.to_owned())
}

fn discover_public_keys() -> Result<Vec<String>> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let directory = home.join(".ssh");
    let entries = std::fs::read_dir(&directory)
        .with_context(|| format!("read SSH directory {}", directory.display()))?;
    let mut keys = BTreeSet::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("read {} entry", directory.display()))?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("pub")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let value = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read public key {}", entry.path().display()))?;
        let mut key = PublicKey::from_openssh(value.trim())
            .with_context(|| format!("parse public key {}", entry.path().display()))?;
        key.set_comment("");
        keys.insert(key.to_openssh()?);
    }
    if keys.is_empty() {
        bail!("no valid public keys found in {}", directory.display());
    }
    Ok(keys.into_iter().collect())
}
