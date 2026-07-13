use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DEVCONTAINER_CLI_VERSION: &str = "0.80.2";
const COMMON_PACKAGES: &[&str] = &[
    "ca-certificates",
    "docker.io",
    "docker-buildx",
    "docker-compose-v2",
    "git",
    "openssh-server",
    "nodejs",
    "npm",
];

pub type PackageVersions = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapPolicy {
    pub session: SessionFrontend,
    pub packages: PackageVersions,
    pub devcontainer_cli_version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrontend {
    Tmux,
    Byobu,
}

impl BootstrapPolicy {
    pub fn expected_package_names(session: SessionFrontend) -> Vec<&'static str> {
        let mut packages = COMMON_PACKAGES.to_vec();
        packages.push(match session {
            SessionFrontend::Tmux => "tmux",
            SessionFrontend::Byobu => "byobu",
        });
        if session == SessionFrontend::Byobu {
            packages.push("tmux");
        }
        packages
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.devcontainer_cli_version != DEVCONTAINER_CLI_VERSION {
            return Err(format!(
                "unsupported Dev Container CLI version {}; expected {DEVCONTAINER_CLI_VERSION}",
                self.devcontainer_cli_version
            ));
        }
        let expected = Self::expected_package_names(self.session)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let actual = self
            .packages
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if expected != actual {
            let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
            let unexpected = actual.difference(&expected).copied().collect::<Vec<_>>();
            let mut differences = Vec::new();
            if !missing.is_empty() {
                differences.push(format!("missing {}", missing.join(", ")));
            }
            if !unexpected.is_empty() {
                differences.push(format!("unexpected {}", unexpected.join(", ")));
            }
            return Err(format!(
                "bootstrap package manifest differs from policy: {}",
                differences.join("; ")
            ));
        }
        if let Some(name) = self
            .packages
            .iter()
            .find_map(|(name, version)| version.is_empty().then_some(name))
        {
            return Err(format!("bootstrap package version is empty for {name}"));
        }
        Ok(())
    }

    pub(crate) fn pinned_packages(&self) -> Vec<String> {
        self.packages
            .iter()
            .map(|(name, version)| format!("{name}={version}"))
            .collect()
    }
}
