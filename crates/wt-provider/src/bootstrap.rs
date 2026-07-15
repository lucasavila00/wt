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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSet {
    names: Vec<&'static str>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapPolicy {
    pub packages: PackageVersions,
    pub devcontainer_cli_version: String,
}

impl BootstrapPolicy {
    pub fn from_installed_packages(
        installed: PackageVersions,
        devcontainer_cli_version: String,
        machine_packages: &[&'static str],
    ) -> Result<Self, String> {
        let complete = PackageSet::provisioner().with_packages(machine_packages)?;
        complete.validate_versions(&installed)?;
        let required = PackageSet::provisioner()
            .names()
            .iter()
            .map(|name| {
                installed
                    .get(*name)
                    .map(|version| ((*name).to_owned(), version.clone()))
                    .ok_or_else(|| format!("installed package manifest is missing {name}"))
            })
            .collect::<Result<PackageVersions, _>>()?;
        let policy = Self {
            packages: required,
            devcontainer_cli_version,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.devcontainer_cli_version != DEVCONTAINER_CLI_VERSION {
            return Err(format!(
                "unsupported Dev Container CLI version {}; expected {DEVCONTAINER_CLI_VERSION}",
                self.devcontainer_cli_version
            ));
        }
        let expected = PackageSet::provisioner()
            .names
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

impl PackageSet {
    pub fn provisioner() -> Self {
        let mut names = COMMON_PACKAGES.to_vec();
        names.push("byobu");
        names.push("tmux");
        Self { names }
    }

    pub fn with_packages(mut self, packages: &[&'static str]) -> Result<Self, String> {
        for package in packages {
            if package.is_empty()
                || !package.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'+' | b'-' | b'.')
                })
            {
                return Err(format!("invalid package name: {package}"));
            }
            if self.names.contains(package) {
                return Err(format!("duplicate package in policy: {package}"));
            }
            self.names.push(package);
        }
        Ok(self)
    }

    pub fn names(&self) -> &[&'static str] {
        &self.names
    }

    pub fn parse_versions(&self, text: &str) -> Result<PackageVersions, String> {
        let mut packages = PackageVersions::new();
        for (index, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let (name, version) = line.split_once('\t').ok_or_else(|| {
                format!(
                    "malformed installed package manifest line {}: expected name<TAB>version",
                    index + 1
                )
            })?;
            if name.is_empty() || version.is_empty() || version.contains('\t') {
                return Err(format!(
                    "malformed installed package manifest line {}: expected name<TAB>version",
                    index + 1
                ));
            }
            if packages
                .insert(name.to_owned(), version.to_owned())
                .is_some()
            {
                return Err(format!(
                    "duplicate installed package manifest entry: {name}"
                ));
            }
        }
        self.validate_versions(&packages)?;
        Ok(packages)
    }

    pub fn validate_versions(&self, packages: &PackageVersions) -> Result<(), String> {
        let expected = self.names.iter().copied().collect::<BTreeSet<_>>();
        let actual = packages.keys().map(String::as_str).collect::<BTreeSet<_>>();
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).copied().collect::<Vec<_>>();
        if !missing.is_empty() || !unexpected.is_empty() {
            let mut differences = Vec::new();
            if !missing.is_empty() {
                differences.push(format!("missing {}", missing.join(", ")));
            }
            if !unexpected.is_empty() {
                differences.push(format!("unexpected {}", unexpected.join(", ")));
            }
            return Err(format!(
                "installed package manifest differs from policy: {}",
                differences.join("; ")
            ));
        }
        if let Some(name) = packages
            .iter()
            .find_map(|(name, version)| version.is_empty().then_some(name))
        {
            return Err(format!(
                "installed package manifest has an empty version for {name}"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE_PACKAGES: &[&str] = &["qemu-guest-agent"];

    fn installed_versions() -> PackageVersions {
        PackageSet::provisioner()
            .with_packages(MACHINE_PACKAGES)
            .unwrap()
            .names()
            .iter()
            .map(|name| ((*name).to_owned(), "1:2.3-4".to_owned()))
            .collect()
    }

    #[test]
    fn installed_image_and_runtime_policy_share_one_package_set() {
        let installed = installed_versions();
        let policy = BootstrapPolicy::from_installed_packages(
            installed,
            DEVCONTAINER_CLI_VERSION.to_owned(),
            MACHINE_PACKAGES,
        )
        .unwrap();

        assert_eq!(
            policy.packages.keys().cloned().collect::<Vec<_>>(),
            [
                "byobu",
                "ca-certificates",
                "docker-buildx",
                "docker-compose-v2",
                "docker.io",
                "git",
                "nodejs",
                "npm",
                "openssh-server",
                "tmux",
            ]
        );
        assert!(!policy.packages.contains_key("qemu-guest-agent"));
    }

    #[test]
    fn installed_package_drift_is_rejected_before_runtime_policy_is_built() {
        let mut installed = installed_versions();
        installed.remove("docker-buildx");
        installed.insert("screen".to_owned(), "1".to_owned());

        let error = BootstrapPolicy::from_installed_packages(
            installed,
            DEVCONTAINER_CLI_VERSION.to_owned(),
            MACHINE_PACKAGES,
        )
        .unwrap_err();
        insta::assert_snapshot!(error, @"installed package manifest differs from policy: missing docker-buildx; unexpected screen");
    }

    #[test]
    fn installed_package_manifest_parser_is_strict() {
        let packages = PackageSet::provisioner()
            .with_packages(MACHINE_PACKAGES)
            .unwrap();
        assert!(packages.parse_versions("tmux=1\n").is_err());
        assert!(packages.parse_versions("tmux\t1\ntmux\t2\n").is_err());
    }
}
