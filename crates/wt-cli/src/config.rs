use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientConfig {
    pub contexts: Vec<Context>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Context {
    pub name: String,
    pub kind: ContextKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextKind {
    BareMetalLocal,
    BareMetalSsh { host: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    version: u32,
    contexts: Vec<ContextFile>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ContextFile {
    BareMetalLocal { name: String },
    BareMetalSsh { name: String, host: String },
}

impl ClientConfig {
    pub fn load() -> Result<Self> {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Self::load_from(&PathBuf::from(home).join(".wt/config.toml"))
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("read client config {}", path.display()))?;
        let file: ConfigFile = toml::from_str(&contents)
            .with_context(|| format!("parse client config {}", path.display()))?;
        if file.version != 1 {
            bail!(
                "unsupported client config version {}; expected 1",
                file.version
            );
        }
        if file.contexts.is_empty() {
            bail!("client config must contain at least one context");
        }
        let mut names = HashSet::new();
        let mut contexts = Vec::with_capacity(file.contexts.len());
        for entry in file.contexts {
            let (name, kind) = match entry {
                ContextFile::BareMetalLocal { name } => (name, ContextKind::BareMetalLocal),
                ContextFile::BareMetalSsh { name, host } => {
                    if host.is_empty() || host.starts_with('-') {
                        bail!("context {name}: SSH host must not be empty or start with '-'");
                    }
                    (name, ContextKind::BareMetalSsh { host })
                }
            };
            validate_context_name(&name)?;
            if !names.insert(name.clone()) {
                bail!("duplicate context name: {name}");
            }
            contexts.push(Context { name, kind });
        }
        Ok(Self { contexts })
    }

    pub fn context(&self, name: &str) -> Option<&Context> {
        self.contexts.iter().find(|context| context.name == name)
    }
}

fn validate_context_name(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 63 {
        bail!("invalid context name {value:?}: must contain 1 to 63 characters");
    }
    let valid_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !valid_edge(value.as_bytes()[0]) || !valid_edge(value.as_bytes()[value.len() - 1]) {
        bail!(
            "invalid context name {value:?}: must start and end with a lowercase letter or digit"
        );
    }
    if !value.bytes().all(|byte| valid_edge(byte) || byte == b'-') {
        bail!("invalid context name {value:?}: only lowercase letters, digits, and hyphens are allowed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_mixed_contexts() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n[[contexts]]\nname = \"lab\"\nkind = \"bare_metal_ssh\"\nhost = \"wt-lab\"\n",
        )
        .unwrap();
        let config = ClientConfig::load_from(&path).unwrap();
        assert_eq!(config.contexts.len(), 2);
        assert_eq!(
            config.contexts[1].kind,
            ContextKind::BareMetalSsh {
                host: "wt-lab".into()
            }
        );
    }

    #[test]
    fn rejects_invalid_configs() {
        for contents in [
            "version = 1\ncontexts = []\n",
            "version = 2\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
            "version = 1\n[[contexts]]\nname = \"bad.name\"\nkind = \"bare_metal_local\"\n",
            "version = 1\n[[contexts]]\nname = \"same\"\nkind = \"bare_metal_local\"\n[[contexts]]\nname = \"same\"\nkind = \"bare_metal_local\"\n",
            "version = 1\n[[contexts]]\nname = \"lab\"\nkind = \"bare_metal_ssh\"\nhost = \"-bad\"\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("config.toml");
            std::fs::write(&path, contents).unwrap();
            assert!(ClientConfig::load_from(&path).is_err(), "{contents}");
        }
    }
}
