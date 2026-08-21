use anyhow::{bail, Context as _, Result};
use clap::{Args, Subcommand};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Subcommand)]
pub(super) enum NewKind {
    /// Create a raw Ubuntu world from cloud-init user-data.
    Host(NewHost),
}

#[derive(Debug, Args)]
pub(super) struct NewHost {
    /// Name of the world to create.
    pub(super) name: wt_control_protocol::InstanceName,
    /// Override the default cloud-init user-data file.
    #[arg(long, value_name = "FILE")]
    pub(super) user_data: Option<PathBuf>,
}

pub(super) struct Input {
    pub(super) name: wt_control_protocol::InstanceName,
    pub(super) user_data: String,
    pub(super) user_data_path: PathBuf,
}

impl NewHost {
    pub(super) fn load(self) -> Result<Input> {
        let (user_data_path, is_default) = match self.user_data {
            Some(path) => (path, false),
            None => (
                default_user_data_path(
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    std::env::var_os("HOME").as_deref(),
                )?,
                true,
            ),
        };
        let user_data = if is_default {
            read_user_data(&user_data_path).with_context(|| {
                format!(
                    "default cloud-init user-data is unavailable; create {} or pass `--user-data FILE`",
                    user_data_path.display()
                )
            })?
        } else {
            read_user_data(&user_data_path)?
        };
        Ok(Input {
            name: self.name,
            user_data,
            user_data_path,
        })
    }
}

fn read_user_data(path: &Path) -> Result<String> {
    let user_data = std::fs::read_to_string(path)
        .with_context(|| format!("read cloud-init user-data {}", path.display()))?;
    if user_data.is_empty() {
        bail!("cloud-init user-data {} is empty", path.display());
    }
    Ok(user_data)
}

pub(super) fn application_summary(path: &Path) -> String {
    format!("Kind        host\nCloud-init  {}\n", path.display())
}

fn default_user_data_path(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf> {
    let config_home = match xdg_config_home.filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(home.context("HOME is not set")?).join(".config"),
    };
    Ok(config_home.join("wt/cloud-init.yaml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_recipe_from_xdg_or_home() {
        assert_eq!(
            default_user_data_path(Some(OsStr::new("/xdg")), Some(OsStr::new("/home/test")))
                .unwrap(),
            PathBuf::from("/xdg/wt/cloud-init.yaml")
        );
        assert_eq!(
            default_user_data_path(None, Some(OsStr::new("/home/test"))).unwrap(),
            PathBuf::from("/home/test/.config/wt/cloud-init.yaml")
        );
    }

    #[test]
    fn review_shows_cloud_init_source() {
        insta::assert_snapshot!(
            application_summary(Path::new("/home/test/.config/wt/cloud-init.yaml")),
            @r###"
        Kind        host
        Cloud-init  /home/test/.config/wt/cloud-init.yaml
        "###
        );
    }
}
