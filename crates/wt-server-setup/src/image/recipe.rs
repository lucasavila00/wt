use anyhow::{Error, Result};
use wt_devcontainer::{PackageSet, PackageVersions, DEVCONTAINER_CLI_VERSION};

pub(super) const RECIPE_VERSION: u32 = 1;
pub(super) const BYOBU_VERSION: &str = "7.15-0ubuntu1";
pub(super) const BYOBU_DEB: &str = "byobu_7.15-0ubuntu1_all.deb";
pub(super) const BYOBU_SHA256: &str =
    "7ed723668e47f44cf6a066ace1ca801dd60e732404213856ac2bfa4d1eb352fc";
pub(super) const BYOBU_URL: &str =
    "https://snapshot.ubuntu.com/ubuntu/20260710T120000Z/pool/main/b/byobu/byobu_7.15-0ubuntu1_all.deb";
pub(super) const TMUX_VERSION: &str = "3.6b";
pub(super) const TMUX_SHA256: &str =
    "390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7";
pub(super) const NCURSES_TERM_DEB: &str = "ncurses-term_6.6+20260608-2_all.deb";
pub(super) const NCURSES_TERM_SHA256: &str =
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
        r#"#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
runcmd:
  - /bin/sh /var/tmp/wt-image-build.sh
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"#
        .to_owned()
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

pub(super) fn build_environment(
    kind: &str,
    recipe_version: u32,
    tmux_config_sha256: &str,
    byobu_color_sha256: &str,
) -> String {
    format!(
        "WT_IMAGE_KIND='{}'\nWT_IMAGE_RECIPE_VERSION='{}'\nBYOBU_VERSION='{}'\nBYOBU_SHA256='{}'\nTMUX_VERSION='{}'\nTMUX_SHA256='{}'\nNCURSES_TERM_DEB='{}'\nNCURSES_TERM_SHA256='{}'\nGHOSTTY_TERMINFO_SHA256='{}'\nTMUX_CONFIG_SHA256='{}'\nBYOBU_COLOR_SHA256='{}'\nDEVCONTAINER_CLI_VERSION='{}'\n",
        kind,
        recipe_version,
        BYOBU_VERSION,
        BYOBU_SHA256,
        TMUX_VERSION,
        TMUX_SHA256,
        NCURSES_TERM_DEB,
        NCURSES_TERM_SHA256,
        GHOSTTY_TERMINFO_SHA256,
        tmux_config_sha256,
        byobu_color_sha256,
        DEVCONTAINER_CLI_VERSION,
    )
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
    fn renders_shared_image_cloud_config() {
        insta::assert_snapshot!(
            ImageRecipe::new().cloud_config(),
            @r###"
#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
runcmd:
  - /bin/sh /var/tmp/wt-image-build.sh
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
