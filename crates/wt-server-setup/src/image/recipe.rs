use anyhow::{Error, Result};
use wt_provider::{PackageSet, PackageVersions, DEVCONTAINER_CLI_VERSION};

pub(super) const RECIPE_VERSION: u32 = 1;
pub(super) const TMUX_VERSION: &str = "3.6b";
const TMUX_SHA256: &str = "390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7";

pub(super) struct ImageRecipe {
    packages: PackageSet,
}

impl ImageRecipe {
    pub(super) fn new() -> Self {
        let packages = PackageSet::provisioner()
            .with_packages(wt_libvirt::MACHINE_BOOTSTRAP_PACKAGES)
            .expect("libvirt machine package policy must be valid");
        Self { packages }
    }

    pub(super) fn devcontainer_cli_version(&self) -> &'static str {
        DEVCONTAINER_CLI_VERSION
    }

    pub(super) fn cloud_config(&self) -> String {
        let requested_packages = self
            .packages
            .names()
            .iter()
            .map(|package| format!("  - {package}"))
            .collect::<Vec<_>>()
            .join("\n");
        let verified_packages = self.packages.names().join(" ");
        let devcontainer_cli = self.devcontainer_cli_version();
        let tmux_version = TMUX_VERSION;
        let tmux_sha256 = TMUX_SHA256;

        format!(
            r#"#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
bootcmd:
  - echo 'WT_IMAGE_PHASE=updating package indexes and installing guest packages' > /dev/ttyS0
package_update: true
packages:
{requested_packages}
  - bison
  - build-essential
  - curl
  - libevent-dev
  - libncurses-dev
  - pkg-config
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@{devcontainer_cli}
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=installing tmux {tmux_version}' > /dev/ttyS0
  - curl -fL --output /tmp/tmux.tar.gz https://github.com/tmux/tmux/releases/download/{tmux_version}/tmux-{tmux_version}.tar.gz && printf '%s  %s\n' {tmux_sha256} /tmp/tmux.tar.gz | sha256sum --check --strict && tar -xzf /tmp/tmux.tar.gz -C /tmp && cd /tmp/tmux-{tmux_version} && ./configure --prefix=/usr && make -j2 && make install && install -m 0755 /usr/bin/tmux /var/lib/wt-tmux && test "$(/var/lib/wt-tmux -V)" = 'tmux {tmux_version}' && cd / && rm -rf /tmp/tmux.tar.gz /tmp/tmux-{tmux_version} && printf 'ready\n' > /var/lib/wt-tmux-ready
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
        self.packages.parse_versions(text).map_err(Error::msg)
    }

    pub(super) fn validate_package_versions(&self, packages: &PackageVersions) -> Result<()> {
        self.packages
            .validate_versions(packages)
            .map_err(Error::msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_output(recipe: &ImageRecipe) -> String {
        recipe
            .packages
            .names()
            .iter()
            .rev()
            .map(|name| format!("{name}\t1:2.3-4"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_byobu_cloud_config() {
        insta::assert_snapshot!(
            ImageRecipe::new().cloud_config(),
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
  - bison
  - build-essential
  - curl
  - libevent-dev
  - libncurses-dev
  - pkg-config
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=installing tmux 3.6b' > /dev/ttyS0
  - curl -fL --output /tmp/tmux.tar.gz https://github.com/tmux/tmux/releases/download/3.6b/tmux-3.6b.tar.gz && printf '%s  %s\n' 390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7 /tmp/tmux.tar.gz | sha256sum --check --strict && tar -xzf /tmp/tmux.tar.gz -C /tmp && cd /tmp/tmux-3.6b && ./configure --prefix=/usr && make -j2 && make install && install -m 0755 /usr/bin/tmux /var/lib/wt-tmux && test "$(/var/lib/wt-tmux -V)" = 'tmux 3.6b' && cd / && rm -rf /tmp/tmux.tar.gz /tmp/tmux-3.6b && printf 'ready\n' > /var/lib/wt-tmux-ready
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
        let recipe = ImageRecipe::new();
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert_eq!(packages["tmux"], "1:2.3-4");
        assert_eq!(packages.len(), 11);
    }

    #[test]
    fn byobu_requires_its_tmux_backend() {
        let recipe = ImageRecipe::new();
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert!(packages.contains_key("byobu"));
        assert!(packages.contains_key("tmux"));
        assert_eq!(packages.len(), 11);
    }

    #[test]
    fn reports_missing_and_unexpected_packages() {
        let recipe = ImageRecipe::new();
        let mut packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        packages.remove("tmux");
        packages.insert("screen".to_owned(), "4.9.1".to_owned());

        let error = recipe.validate_package_versions(&packages).unwrap_err();
        insta::assert_snapshot!(error.to_string(), @"installed package manifest differs from policy: missing tmux; unexpected screen");
    }

    #[test]
    fn rejects_duplicate_malformed_and_empty_versions() {
        let recipe = ImageRecipe::new();
        assert!(recipe.parse_package_versions("tmux\t1\ntmux\t2\n").is_err());
        assert!(recipe.parse_package_versions("tmux=1\n").is_err());
        assert!(recipe.parse_package_versions("tmux\t\n").is_err());

        let mut packages = PackageVersions::new();
        for name in recipe.packages.names() {
            packages.insert((*name).to_owned(), "1".to_owned());
        }
        packages.insert("tmux".to_owned(), String::new());
        assert_eq!(
            recipe
                .validate_package_versions(&packages)
                .unwrap_err()
                .to_string(),
            "installed package manifest has an empty version for tmux"
        );
    }
}
