use anyhow::{Error, Result};
use wt_devcontainer::{PackageSet, PackageVersions, DEVCONTAINER_CLI_VERSION};

pub(super) const RECIPE_VERSION: u32 = 3;
const BYOBU_VERSION: &str = "7.15-0ubuntu1";
pub(super) const BYOBU_DEB: &str = "byobu_7.15-0ubuntu1_all.deb";
pub(super) const BYOBU_SHA256: &str =
    "7ed723668e47f44cf6a066ace1ca801dd60e732404213856ac2bfa4d1eb352fc";
pub(super) const BYOBU_URL: &str =
    "https://snapshot.ubuntu.com/ubuntu/20260710T120000Z/pool/main/b/byobu/byobu_7.15-0ubuntu1_all.deb";
pub(super) const TMUX_VERSION: &str = "3.6b";
const TMUX_SHA256: &str = "390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7";
const NCURSES_TERM_DEB: &str = "ncurses-term_6.6+20260608-2_all.deb";
const NCURSES_TERM_SHA256: &str =
    "2696f4d2430b44c1ed25dd20fe91c6bf9811194bfb19a6c5408c83789f9f0cb4";
pub(super) const GHOSTTY_TERMINFO_SHA256: &str =
    "1fbbc41e609831f9847143f368f46fb63fbeef3a1a36ac435dc2c94ec6cc70fa";

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
            .filter(|package| **package != "byobu")
            .map(|package| format!("  - {package}"))
            .collect::<Vec<_>>()
            .join("\n");
        let verified_packages = self.packages.names().join(" ");
        let devcontainer_cli = self.devcontainer_cli_version();
        let byobu_version = BYOBU_VERSION;
        let byobu_sha256 = BYOBU_SHA256;
        let tmux_version = TMUX_VERSION;
        let tmux_sha256 = TMUX_SHA256;
        let ncurses_term_deb = NCURSES_TERM_DEB;
        let ncurses_term_sha256 = NCURSES_TERM_SHA256;
        let ghostty_terminfo_sha256 = GHOSTTY_TERMINFO_SHA256;

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
  - set -eux
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing Byobu {byobu_version}' > /dev/ttyS0
  - printf '%s  %s\n' {byobu_sha256} /var/tmp/wt-byobu.deb | sha256sum --check --strict && apt-get install -y --no-install-recommends /var/tmp/wt-byobu.deb && test "$(dpkg-query -W -f='${{Version}}' byobu)" = '{byobu_version}' && rm -f /var/tmp/wt-byobu.deb && printf 'ready\n' > /var/lib/wt-byobu-ready
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@{devcontainer_cli}
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=installing tmux {tmux_version}' > /dev/ttyS0
  - curl -fL --output /tmp/tmux.tar.gz https://github.com/tmux/tmux/releases/download/{tmux_version}/tmux-{tmux_version}.tar.gz && printf '%s  %s\n' {tmux_sha256} /tmp/tmux.tar.gz | sha256sum --check --strict && tar -xzf /tmp/tmux.tar.gz -C /tmp && cd /tmp/tmux-{tmux_version} && ./configure --prefix=/usr && make -j2 && make install && install -m 0755 /usr/bin/tmux /var/lib/wt-tmux && test "$(/var/lib/wt-tmux -V)" = 'tmux {tmux_version}' && cd / && rm -rf /tmp/tmux.tar.gz /tmp/tmux-{tmux_version} && printf 'ready\n' > /var/lib/wt-tmux-ready
  - echo 'WT_IMAGE_PHASE=installing Ghostty terminfo' > /dev/ttyS0
  - curl -fL --output /tmp/ncurses-term.deb https://archive.ubuntu.com/ubuntu/pool/main/n/ncurses/{ncurses_term_deb} && printf '%s  %s\n' {ncurses_term_sha256} /tmp/ncurses-term.deb | sha256sum --check --strict && install -d -m 0755 /usr/share/terminfo/g /usr/share/terminfo/x && dpkg-deb --fsys-tarfile /tmp/ncurses-term.deb | tar -xO ./usr/share/terminfo/g/ghostty > /usr/share/terminfo/g/ghostty && cp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && printf '%s  %s\n' {ghostty_terminfo_sha256} /usr/share/terminfo/g/ghostty | sha256sum --check --strict && cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && TERM=ghostty tput colors > /dev/null && TERM=xterm-ghostty tput colors > /dev/null && rm -f /tmp/ncurses-term.deb && printf 'ready\n' > /var/lib/wt-ghostty-terminfo-ready
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
        let packages = self.packages.parse_versions(text).map_err(Error::msg)?;
        self.validate_package_versions(&packages)?;
        Ok(packages)
    }

    pub(super) fn validate_package_versions(&self, packages: &PackageVersions) -> Result<()> {
        self.packages
            .validate_versions(packages)
            .map_err(Error::msg)?;
        if packages["byobu"] != BYOBU_VERSION {
            return Err(Error::msg(format!(
                "installed byobu version is {}; expected {BYOBU_VERSION}",
                packages["byobu"]
            )));
        }
        Ok(())
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
            .map(|name| {
                let version = if *name == "byobu" {
                    BYOBU_VERSION
                } else {
                    "1:2.3-4"
                };
                format!("{name}\t{version}")
            })
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
  - tmux
  - qemu-guest-agent
  - bison
  - build-essential
  - curl
  - libevent-dev
  - libncurses-dev
  - pkg-config
runcmd:
  - set -eux
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker buildx version
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing Byobu 7.15-0ubuntu1' > /dev/ttyS0
  - printf '%s  %s\n' 7ed723668e47f44cf6a066ace1ca801dd60e732404213856ac2bfa4d1eb352fc /var/tmp/wt-byobu.deb | sha256sum --check --strict && apt-get install -y --no-install-recommends /var/tmp/wt-byobu.deb && test "$(dpkg-query -W -f='${Version}' byobu)" = '7.15-0ubuntu1' && rm -f /var/tmp/wt-byobu.deb && printf 'ready\n' > /var/lib/wt-byobu-ready
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=installing tmux 3.6b' > /dev/ttyS0
  - curl -fL --output /tmp/tmux.tar.gz https://github.com/tmux/tmux/releases/download/3.6b/tmux-3.6b.tar.gz && printf '%s  %s\n' 390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7 /tmp/tmux.tar.gz | sha256sum --check --strict && tar -xzf /tmp/tmux.tar.gz -C /tmp && cd /tmp/tmux-3.6b && ./configure --prefix=/usr && make -j2 && make install && install -m 0755 /usr/bin/tmux /var/lib/wt-tmux && test "$(/var/lib/wt-tmux -V)" = 'tmux 3.6b' && cd / && rm -rf /tmp/tmux.tar.gz /tmp/tmux-3.6b && printf 'ready\n' > /var/lib/wt-tmux-ready
  - echo 'WT_IMAGE_PHASE=installing Ghostty terminfo' > /dev/ttyS0
  - curl -fL --output /tmp/ncurses-term.deb https://archive.ubuntu.com/ubuntu/pool/main/n/ncurses/ncurses-term_6.6+20260608-2_all.deb && printf '%s  %s\n' 2696f4d2430b44c1ed25dd20fe91c6bf9811194bfb19a6c5408c83789f9f0cb4 /tmp/ncurses-term.deb | sha256sum --check --strict && install -d -m 0755 /usr/share/terminfo/g /usr/share/terminfo/x && dpkg-deb --fsys-tarfile /tmp/ncurses-term.deb | tar -xO ./usr/share/terminfo/g/ghostty > /usr/share/terminfo/g/ghostty && cp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && printf '%s  %s\n' 1fbbc41e609831f9847143f368f46fb63fbeef3a1a36ac435dc2c94ec6cc70fa /usr/share/terminfo/g/ghostty | sha256sum --check --strict && cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && TERM=ghostty tput colors > /dev/null && TERM=xterm-ghostty tput colors > /dev/null && rm -f /tmp/ncurses-term.deb && printf 'ready\n' > /var/lib/wt-ghostty-terminfo-ready
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
    fn rejects_unexpected_byobu_version() {
        let recipe = ImageRecipe::new();
        let mut packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        packages.insert("byobu".to_owned(), "6.11-0ubuntu1".to_owned());

        assert_eq!(
            recipe
                .validate_package_versions(&packages)
                .unwrap_err()
                .to_string(),
            "installed byobu version is 6.11-0ubuntu1; expected 7.15-0ubuntu1"
        );
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
