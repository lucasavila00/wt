use anyhow::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

pub(super) type PackageVersions = BTreeMap<String, String>;

const IMAGE_PACKAGES: &[&str] = &["ca-certificates", "git", "openssh-server", "byobu", "tmux"];
const DEVELOPMENT_TOOL_PACKAGES: &[&str] = &[
    "bison",
    "build-essential",
    "cmake",
    "clang",
    "curl",
    "wget",
    "jq",
    "yq",
    "pkg-config",
    "docker.io",
    "docker-compose-v2",
    "shellcheck",
];
const DEVELOPMENT_TOOLS: &[&str] = &[
    "cargo",
    "rustc",
    "go",
    "python",
    "nvm",
    "node",
    "npm",
    "npx",
    "corepack",
    "shellcheck",
    "uv",
    "docker",
    "docker-compose",
];

struct PackageSet {
    names: Vec<&'static str>,
}

impl PackageSet {
    fn image() -> Self {
        Self {
            names: IMAGE_PACKAGES.to_vec(),
        }
    }

    fn development_tools() -> Self {
        Self {
            names: DEVELOPMENT_TOOLS.to_vec(),
        }
    }

    fn with_packages(mut self, packages: &[&'static str]) -> Result<Self, String> {
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

    #[cfg(test)]
    fn names(&self) -> &[&'static str] {
        &self.names
    }

    fn parse_versions(&self, text: &str) -> Result<PackageVersions, String> {
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

    fn validate_versions(&self, packages: &PackageVersions) -> Result<(), String> {
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

pub(super) fn node_version() -> &'static str {
    include_str!("../../../../../../.nvmrc").trim()
}

pub(super) struct ImageRecipe {
    packages: PackageSet,
    development_tools: PackageSet,
}

impl ImageRecipe {
    pub(super) fn new() -> Self {
        let packages = PackageSet::image()
            .with_packages(wt_libvirt_kvm::MACHINE_BOOTSTRAP_PACKAGES)
            .expect("libvirt machine package policy must be valid")
            .with_packages(DEVELOPMENT_TOOL_PACKAGES)
            .expect("development tool package policy must be valid");
        Self {
            packages,
            development_tools: PackageSet::development_tools(),
        }
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

    pub(super) fn parse_development_tool_versions(&self, text: &str) -> Result<PackageVersions> {
        self.development_tools
            .parse_versions(text)
            .map_err(Error::msg)
    }

    pub(super) fn validate_development_tool_versions(&self, tools: &PackageVersions) -> Result<()> {
        self.development_tools
            .validate_versions(tools)
            .map_err(Error::msg)
    }
}

pub(super) struct BuildEnvironment<'a> {
    pub(super) kind: &'a str,
    pub(super) node_version: &'a str,
    pub(super) tmux_config_sha256: &'a str,
    pub(super) byobu_color_sha256: &'a str,
    pub(super) access_sha256: &'a str,
    pub(super) git_author_sha256: &'a str,
    pub(super) agent_tools_sha256: &'a str,
    pub(super) mount_codex_sha256: &'a str,
}

impl BuildEnvironment<'_> {
    pub(super) fn render(&self) -> String {
        format!(
        "WT_IMAGE_KIND='{}'\nWT_USER='{}'\nWT_GROUP='{}'\nWT_UID='{}'\nWT_GID='{}'\nWT_HOME='{}'\nNODE_VERSION='{}'\nBYOBU_VERSION='{}'\nBYOBU_SHA256='{}'\nTMUX_VERSION='{}'\nTMUX_SHA256='{}'\nNCURSES_TERM_DEB='{}'\nNCURSES_TERM_SHA256='{}'\nGHOSTTY_TERMINFO_SHA256='{}'\nTMUX_CONFIG_SHA256='{}'\nBYOBU_COLOR_SHA256='{}'\nACCESS_SHA256='{}'\nGIT_AUTHOR_SHA256='{}'\nAGENT_TOOLS_SHA256='{}'\nMOUNT_CODEX_SHA256='{}'\n",
        self.kind,
        wt_guest::GUEST_USER,
        wt_guest::GUEST_GROUP,
        wt_guest::GUEST_UID,
        wt_guest::GUEST_GID,
        wt_guest::GUEST_HOME,
        self.node_version,
        BYOBU_VERSION,
        BYOBU_SHA256,
        TMUX_VERSION,
        TMUX_SHA256,
        NCURSES_TERM_DEB,
        NCURSES_TERM_SHA256,
        GHOSTTY_TERMINFO_SHA256,
        self.tmux_config_sha256,
        self.byobu_color_sha256,
        self.access_sha256,
        self.git_author_sha256,
        self.agent_tools_sha256,
        self.mount_codex_sha256,
    ) + &format!(
        "CODEX_RELEASE='{}'\nAGAPI_CODEX_RELEASE='{}'\n",
        include_str!("../../../../../../assets/world/guest/codex-version").trim(),
        include_str!("../../../../agapi/codex-version").trim(),
    )
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
    fn renders_complete_shared_image_environment() {
        insta::assert_snapshot!(
            BuildEnvironment {
                kind: "guest",
                node_version: "24.19.0",
                tmux_config_sha256: "tmux-config-sha",
                byobu_color_sha256: "byobu-color-sha",
                access_sha256: "access-sha",
                git_author_sha256: "git-author-sha",
                agent_tools_sha256: "agent-tools-sha",
                mount_codex_sha256: "mount-codex-sha",
            }
            .render()
            .replace(
                &format!("AGAPI_CODEX_RELEASE='{}'", include_str!("../../../../agapi/codex-version").trim()),
                "AGAPI_CODEX_RELEASE='[agapi-codex-version]'",
            )
            .replace(include_str!("../../../../../../assets/world/guest/codex-version").trim(), "[codex-version]"),
            @r###"
WT_IMAGE_KIND='guest'
WT_USER='wt'
WT_GROUP='wt'
WT_UID='1001'
WT_GID='1001'
WT_HOME='/home/wt'
NODE_VERSION='24.19.0'
BYOBU_VERSION='7.15-0ubuntu1'
BYOBU_SHA256='7ed723668e47f44cf6a066ace1ca801dd60e732404213856ac2bfa4d1eb352fc'
TMUX_VERSION='3.6b'
TMUX_SHA256='390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7'
NCURSES_TERM_DEB='ncurses-term_6.6+20260608-2_all.deb'
NCURSES_TERM_SHA256='2696f4d2430b44c1ed25dd20fe91c6bf9811194bfb19a6c5408c83789f9f0cb4'
GHOSTTY_TERMINFO_SHA256='1fbbc41e609831f9847143f368f46fb63fbeef3a1a36ac435dc2c94ec6cc70fa'
TMUX_CONFIG_SHA256='tmux-config-sha'
BYOBU_COLOR_SHA256='byobu-color-sha'
ACCESS_SHA256='access-sha'
GIT_AUTHOR_SHA256='git-author-sha'
AGENT_TOOLS_SHA256='agent-tools-sha'
MOUNT_CODEX_SHA256='mount-codex-sha'
CODEX_RELEASE='[codex-version]'
AGAPI_CODEX_RELEASE='[agapi-codex-version]'
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
        assert_eq!(packages.len(), 18);
    }

    #[test]
    fn byobu_requires_its_tmux_backend() {
        let recipe = ImageRecipe::new();
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert!(packages.contains_key("byobu"));
        assert!(packages.contains_key("tmux"));
        assert_eq!(packages.len(), 18);
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

    #[test]
    fn development_tool_provenance_is_required() {
        let recipe = ImageRecipe::new();
        let packages = recipe
            .parse_package_versions(&package_output(&recipe))
            .unwrap();
        assert_eq!(packages["shellcheck"], "1:2.3-4");
        let tools = recipe
            .parse_development_tool_versions(
                &DEVELOPMENT_TOOLS
                    .iter()
                    .map(|name| format!("{name}\tversion"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .unwrap();
        recipe.validate_development_tool_versions(&tools).unwrap();
    }
}
