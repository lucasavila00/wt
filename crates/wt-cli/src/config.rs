use anyhow::{bail, Context as AnyhowContext, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub current_context: Option<String>,
    #[serde(default)]
    pub contexts: Vec<Context>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Context {
    pub name: String,
    #[serde(flatten)]
    pub connection: Connection,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Connection {
    BareMetalLocal {
        #[serde(default = "default_helper")]
        helper: String,
        #[serde(default = "default_helper_args")]
        helper_args: Vec<String>,
    },
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default_local());
        }
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        toml::from_str(&contents).with_context(|| format!("parse config {}", path.display()))
    }

    pub fn select(&self, requested: Option<&str>) -> Result<&Context> {
        let name = requested.or(self.current_context.as_deref());
        if let Some(name) = name {
            return self
                .contexts
                .iter()
                .find(|context| context.name == name)
                .with_context(|| format!("context {name:?} not found"));
        }
        match self.contexts.as_slice() {
            [context] => Ok(context),
            [] => bail!("no contexts configured"),
            _ => bail!("multiple contexts configured; set current_context or --context"),
        }
    }

    fn default_local() -> Self {
        Self {
            current_context: Some("local".to_owned()),
            contexts: vec![Context {
                name: "local".to_owned(),
                connection: Connection::BareMetalLocal {
                    helper: std::env::var("WT_HELPER").unwrap_or_else(|_| default_helper()),
                    helper_args: default_helper_args(),
                },
            }],
        }
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("wt/config.toml"));
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/wt/config.toml"))
}

fn default_helper() -> String {
    "wt-local".to_owned()
}

fn default_helper_args() -> Vec<String> {
    vec!["api".to_owned()]
}
