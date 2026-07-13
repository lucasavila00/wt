use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use wt_provider::{BootstrapPolicy, SessionFrontend, DEVCONTAINER_CLI_VERSION};

pub(super) const RECIPE_VERSION: u32 = 1;

pub(super) type PackageVersions = BTreeMap<String, String>;

pub(super) struct ImageRecipe {
    session: SessionFrontend,
}

impl ImageRecipe {
    pub(super) fn new(session: SessionFrontend) -> Self {
        Self { session }
    }

    pub(super) fn devcontainer_cli_version(&self) -> &'static str {
        DEVCONTAINER_CLI_VERSION
    }

    pub(super) fn cloud_config(&self) -> String {
        let requested_packages = self
            .requested_packages()
            .into_iter()
            .map(|package| format!("  - {package}"))
            .collect::<Vec<_>>()
            .join("\n");
        let verified_packages = self.verified_packages().join(" ");
        let devcontainer_cli = self.devcontainer_cli_version();

        format!(
            r#"#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
bootcmd:
  - echo 'WT_IMAGE_PHASE=updating package indexes and installing guest packages' > /dev/ttyS0
package_update: true
packages:
{requested_packages}
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@{devcontainer_cli}
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=recording installed package versions' > /dev/ttyS0
  - dpkg-query -W -f='${{Package}}\t${{Version}}\n' {verified_packages} | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
  - echo 'WT_IMAGE_PHASE=build ready; requesting shutdown' > /dev/ttyS0
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"#
        )
    }

    pub(super) fn parse_package_versions(&self, text: &str) -> Result<PackageVersions> {
        let mut packages = PackageVersions::new();
        for (index, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let (name, version) = line.split_once('\t').ok_or_else(|| {
                anyhow::anyhow!(
                    "malformed image package manifest line {}: expected name<TAB>version",
                    index + 1
                )
            })?;
            if name.is_empty() || version.is_empty() || version.contains('\t') {
                bail!(
                    "malformed image package manifest line {}: expected name<TAB>version",
                    index + 1
                );
            }
            if packages
                .insert(name.to_owned(), version.to_owned())
                .is_some()
            {
                bail!("duplicate image package manifest entry: {name}");
            }
        }
        self.validate_package_versions(&packages)?;
        Ok(packages)
    }

    pub(super) fn validate_package_versions(&self, packages: &PackageVersions) -> Result<()> {
        let expected = self
            .verified_packages()
            .into_iter()
            .collect::<BTreeSet<_>>();
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
            bail!(
                "image package manifest differs from recipe: {}",
                differences.join("; ")
            );
        }
        if let Some(name) = packages
            .iter()
            .find_map(|(name, version)| version.is_empty().then_some(name))
        {
            bail!("image package manifest has an empty version for {name}");
        }
        Ok(())
    }

    fn requested_packages(&self) -> Vec<&'static str> {
        let mut packages = BootstrapPolicy::expected_package_names(self.session);
        packages.push("qemu-guest-agent");
        packages
    }

    fn verified_packages(&self) -> Vec<&'static str> {
        self.requested_packages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_output(recipe: &ImageRecipe) -> String {
        recipe
            .verified_packages()
            .into_iter()
            .rev()
            .map(|name| format!("{name}\t1:2.3-4"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_tmux_cloud_config() {
        insta::assert_snapshot!(
            ImageRecipe::new(SessionFrontend::Tmux).cloud_config(),
            @r###"
#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
bootcmd:
  - echo 'WT_IMAGE_PHASE=updating package indexes and installing guest packages' > /dev/ttyS0
package_update: true
packages:
  - ca-certificates
  - docker.io
  - docker-buildx
  - docker-compose-v2
  - git
  - openssh-server
  - nodejs
  - npm
  - tmux
  - qemu-guest-agent
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=recording installed package versions' > /dev/ttyS0
  - dpkg-query -W -f='${Package}\t${Version}\n' ca-certificates docker.io docker-buildx docker-compose-v2 git openssh-server nodejs npm tmux qemu-guest-agent | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
  - echo 'WT_IMAGE_PHASE=build ready; requesting shutdown' > /dev/ttyS0
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"###
        );
    }

    #[test]
    fn renders_byobu_cloud_config() {
        insta::assert_snapshot!(
            ImageRecipe::new(SessionFrontend::Byobu).cloud_config(),
            @r###"
#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
bootcmd:
  - echo 'WT_IMAGE_PHASE=updating package indexes and installing guest packages' > /dev/ttyS0
package_update: true
packages:
  - ca-certificates
  - docker.io
  - docker-buildx
  - docker-compose-v2
  - git
  - openssh-server
  - nodejs
  - npm
  - byobu
  - tmux
  - qemu-guest-agent
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=recording installed package versions' > /dev/ttyS0
  - dpkg-query -W -f='${Package}\t${Version}\n' ca-certificates docker.io docker-buildx docker-compose-v2 git openssh-server nodejs npm byobu tmux qemu-guest-agent | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
  - echo 'WT_IMAGE_PHASE=build ready; requesting shutdown' > /dev/ttyS0
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"###
        );
    }

    #[test]
    fn parses_unordered_package_versions() {
        let recipe = ImageRecipe::new(SessionFrontend::Tmux);
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert_eq!(packages["tmux"], "1:2.3-4");
        assert_eq!(packages.len(), 10);
    }

    #[test]
    fn byobu_requires_its_tmux_backend() {
        let recipe = ImageRecipe::new(SessionFrontend::Byobu);
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert!(packages.contains_key("byobu"));
        assert!(packages.contains_key("tmux"));
        assert_eq!(packages.len(), 11);
    }

    #[test]
    fn reports_missing_and_unexpected_packages() {
        let recipe = ImageRecipe::new(SessionFrontend::Tmux);
        let mut packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        packages.remove("tmux");
        packages.insert("screen".to_owned(), "4.9.1".to_owned());

        let error = recipe.validate_package_versions(&packages).unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"image package manifest differs from recipe: missing tmux; unexpected screen");
    }

    #[test]
    fn rejects_duplicate_malformed_and_empty_versions() {
        let recipe = ImageRecipe::new(SessionFrontend::Tmux);
        assert!(recipe.parse_package_versions("tmux\t1\ntmux\t2\n").is_err());
        assert!(recipe.parse_package_versions("tmux=1\n").is_err());
        assert!(recipe.parse_package_versions("tmux\t\n").is_err());

        let mut packages = PackageVersions::new();
        for name in recipe.verified_packages() {
            packages.insert(name.to_owned(), "1".to_owned());
        }
        packages.insert("tmux".to_owned(), String::new());
        assert_eq!(
            recipe
                .validate_package_versions(&packages)
                .unwrap_err()
                .to_string(),
            "image package manifest has an empty version for tmux"
        );
    }
}
