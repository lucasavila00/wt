use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use wt_installer_support::cmd;
use wt_installer_support::{sudo_install, sudo_move, Runner};
use wt_server::ServerConfig;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";
const STATIC_BINARIES: [&str; 6] = [
    "wt-agent-tool-gateway",
    "wt-agent-tool-gateway-relay",
    "git-remote-wt-agent",
    "wt-tools",
    "wt",
    "wt-codex-integration",
];

pub(super) fn build(runner: &impl Runner) -> Result<()> {
    runner.run(
        cmd!("cargo", "build", "--quiet", "--release", "-p", "wt-server",),
        "build native wt-server",
    )?;
    build_static(runner)
}

pub(super) fn build_static(runner: &impl Runner) -> Result<()> {
    runner.run(
        cmd!(
            "cargo",
            "build",
            "--quiet",
            "--release",
            "--target",
            MUSL_TARGET,
            "-p",
            "wt-agent-tool-gateway",
            "-p",
            "wt-client",
            "-p",
            "wt-codex-integration",
        ),
        "build static WT binaries",
    )?;
    for name in STATIC_BINARIES {
        validate_static_binary(runner, &release_binary(name), name)?;
    }
    Ok(())
}

pub(super) fn install(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    for name in [
        "wt-agent-tool-gateway",
        "wt-agent-tool-gateway-relay",
        "git-remote-wt-agent",
        "wt-tools",
        "wt",
        "wt-codex-integration",
        "wt-server",
    ] {
        let source = release_binary(name);
        let destination = config.install.binary_dir.join(name);
        let temporary = config.install.binary_dir.join(format!(".{name}.wt-new"));
        if temporary.exists() {
            bail!("stale binary install file exists: {}", temporary.display());
        }
        sudo_install(runner, &source, &temporary, 0o755)?;
        sudo_move(runner, &temporary, &destination)?;
    }
    Ok(())
}

pub(crate) fn release_binary(name: &str) -> PathBuf {
    if STATIC_BINARIES.contains(&name) {
        Path::new("target")
            .join(MUSL_TARGET)
            .join("release")
            .join(name)
    } else {
        Path::new("target/release").join(name)
    }
}

fn validate_static_binary(runner: &impl Runner, path: &Path, name: &str) -> Result<()> {
    let program_headers = runner.text(
        cmd!("readelf", "--program-headers", "--wide", path),
        &format!("inspect {name} program headers"),
    )?;
    let version_info = runner.text(
        cmd!("readelf", "--version-info", "--wide", path),
        &format!("inspect {name} symbol versions"),
    )?;
    validate_static_elf(name, &program_headers, &version_info)
}

fn validate_static_elf(name: &str, program_headers: &str, version_info: &str) -> Result<()> {
    if program_headers.lines().any(|line| line.contains("INTERP")) {
        bail!("{name} is dynamically linked: ELF program interpreter found");
    }
    if version_info.contains("GLIBC_") {
        bail!("{name} is dynamically linked: GLIBC symbol requirement found");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_installed_binaries_except_wt_server_use_the_musl_release_directory() {
        assert_eq!(
            release_binary("wt-tools"),
            Path::new("target/x86_64-unknown-linux-musl/release/wt-tools")
        );
        assert_eq!(
            release_binary("git-remote-wt-agent"),
            Path::new("target/x86_64-unknown-linux-musl/release/git-remote-wt-agent")
        );
        assert_eq!(
            release_binary("wt-agent-tool-gateway-relay"),
            Path::new("target/x86_64-unknown-linux-musl/release/wt-agent-tool-gateway-relay")
        );
        assert_eq!(
            release_binary("wt-agent-tool-gateway"),
            Path::new("target/x86_64-unknown-linux-musl/release/wt-agent-tool-gateway")
        );
        assert_eq!(
            release_binary("wt"),
            Path::new("target/x86_64-unknown-linux-musl/release/wt")
        );
        assert_eq!(
            release_binary("wt-codex-integration"),
            Path::new("target/x86_64-unknown-linux-musl/release/wt-codex-integration")
        );
        assert_eq!(
            release_binary("wt-server"),
            Path::new("target/release/wt-server")
        );
    }

    #[test]
    fn static_binaries_must_be_free_of_dynamic_glibc_requirements() {
        validate_static_elf(
            "wt-tools",
            "ELF program headers\n",
            "No version information\n",
        )
        .unwrap();

        insta::assert_snapshot!(
            validate_static_elf("wt-tools", "  INTERP 0x000000\n", "").unwrap_err(),
            @"wt-tools is dynamically linked: ELF program interpreter found"
        );
        insta::assert_snapshot!(
            validate_static_elf("wt-tools", "", "Name: GLIBC_2.39\n").unwrap_err(),
            @"wt-tools is dynamically linked: GLIBC symbol requirement found"
        );
    }
}
