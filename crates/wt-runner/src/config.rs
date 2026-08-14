use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use wt_libvirt::MachineConfig;
use wt_registry::Resources;

pub const RUNNER_CONFIG_PATH: &str = "/etc/wt/runner.toml";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerConfig {
    pub version: u32,
    pub github: GithubConfig,
    pub image: ImageConfig,
    pub libvirt: LibvirtConfig,
    pub runner: RunnerResourceConfig,
    pub state: StateConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    pub config_url: String,
    pub scale_set_name: String,
    pub runner_group: String,
    pub labels: Vec<String>,
    pub app_client_id: String,
    pub app_installation_id: u64,
    pub max_runners: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageConfig {
    pub installed_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibvirtConfig {
    pub network: String,
    pub runners_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerResourceConfig {
    pub vcpus: u64,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub boot_timeout_seconds: u64,
    pub job_timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    pub database_path: PathBuf,
    pub log_dir: PathBuf,
}

impl RunnerConfig {
    pub fn load() -> Result<Self, String> {
        Self::load_from(Path::new(RUNNER_CONFIG_PATH))
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("read runner config {}: {error}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .map_err(|error| format!("parse runner config {}: {error}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported runner config version {}; expected 1",
                self.version
            ));
        }
        let target = parse_config_url(&self.github.config_url)?;
        if target.organization.is_empty() || target.repository.as_deref() == Some("") {
            return Err("github.config_url must identify an organization or repository".into());
        }
        for (name, value) in [
            ("github.scale_set_name", &self.github.scale_set_name),
            ("github.runner_group", &self.github.runner_group),
            ("github.app_client_id", &self.github.app_client_id),
            ("libvirt.network", &self.libvirt.network),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        if self.github.app_installation_id == 0 || self.github.max_runners == 0 {
            return Err(
                "GitHub App installation ID and max runners must be greater than zero".into(),
            );
        }
        if self.github.labels.is_empty()
            || self
                .github
                .labels
                .iter()
                .any(|label| label.trim().is_empty())
        {
            return Err("github.labels must contain non-empty labels".into());
        }
        let mut unique_labels = self.github.labels.clone();
        unique_labels.sort();
        unique_labels.dedup();
        if unique_labels.len() != self.github.labels.len() {
            return Err("github.labels must not contain duplicates".into());
        }
        for (name, path) in [
            ("image.installed_path", &self.image.installed_path),
            ("libvirt.runners_dir", &self.libvirt.runners_dir),
            ("state.database_path", &self.state.database_path),
            ("state.log_dir", &self.state.log_dir),
        ] {
            validate_path(name, path)?;
        }
        if self
            .image
            .installed_path
            .extension()
            .and_then(|value| value.to_str())
            != Some("qcow2")
        {
            return Err("image.installed_path must end in .qcow2".into());
        }
        if self.runner.vcpus == 0
            || self.runner.memory_mib == 0
            || self.runner.disk_gib == 0
            || self.runner.boot_timeout_seconds == 0
            || self.runner.job_timeout_seconds == 0
        {
            return Err("runner resources and timeouts must be greater than zero".into());
        }
        Ok(())
    }

    pub fn resources(&self) -> Resources {
        Resources {
            vcpus: self.runner.vcpus,
            memory_mib: self.runner.memory_mib,
            disk_gib: self.runner.disk_gib,
        }
    }

    pub fn machine_config(&self) -> MachineConfig {
        MachineConfig {
            image: self.image.installed_path.clone(),
            worlds_dir: self.libvirt.runners_dir.clone(),
            network: self.libvirt.network.clone(),
            boot_timeout: Duration::from_secs(self.runner.boot_timeout_seconds),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GithubTarget {
    pub organization: String,
    pub repository: Option<String>,
}

pub(crate) fn parse_config_url(value: &str) -> Result<GithubTarget, String> {
    let url =
        url::Url::parse(value).map_err(|error| format!("parse github.config_url: {error}"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "github.config_url must be an https://github.com organization or repository URL".into(),
        );
    }
    let parts = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [organization] => Ok(GithubTarget {
            organization: (*organization).to_owned(),
            repository: None,
        }),
        [organization, repository] => Ok(GithubTarget {
            organization: (*organization).to_owned(),
            repository: Some((*repository).to_owned()),
        }),
        _ => Err("github.config_url must identify one organization or repository".into()),
    }
}

fn validate_path(name: &str, path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path == Path::new("/")
        || path
            .components()
            .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
    {
        return Err(format!(
            "{name} must be an absolute normalized path below /"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
version = 1

[github]
config_url = "https://github.com/acme/widgets"
scale_set_name = "wt-kvm"
runner_group = "default"
labels = ["wt-kvm"]
app_client_id = "Iv1.fixture"
app_installation_id = 42
max_runners = 8

[image]
installed_path = "/var/lib/wt/images/runner.qcow2"

[libvirt]
network = "wt-runners"
runners_dir = "/var/lib/libvirt/images/wt-runners"

[runner]
vcpus = 2
memory_mib = 4096
disk_gib = 32
boot_timeout_seconds = 300
job_timeout_seconds = 21600

[state]
database_path = "/var/lib/wt/registry.db"
log_dir = "/var/log/wt/runners"
"#;

    #[test]
    fn complete_config_is_valid() {
        let config: RunnerConfig = toml::from_str(VALID).unwrap();
        config.validate().unwrap();
        assert_eq!(config.resources().memory_mib, 4096);
    }

    #[test]
    fn config_is_strict_and_github_com_only() {
        assert!(toml::from_str::<RunnerConfig>(
            &VALID.replace("version = 1", "version = 1\nunknown = true")
        )
        .is_err());
        let config: RunnerConfig =
            toml::from_str(&VALID.replace("https://github.com", "https://github.example.com"))
                .unwrap();
        assert!(config.validate().is_err());
    }
}
