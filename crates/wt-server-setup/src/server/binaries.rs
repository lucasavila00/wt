use crate::files::{sudo_install, sudo_move};
use crate::runner::Runner;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use wt_command::cmd;
use wt_server::ServerConfig;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";
const DEVCONTAINER_BINARIES: [&str; 2] = ["git-remote-ag", "ag-git"];

pub(super) fn build_and_install(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    runner.run(
        cmd!(
            "cargo",
            "build",
            "--quiet",
            "--release",
            "-p",
            "wt-agent-git",
            "-p",
            "wt-cli",
            "-p",
            "wt-devcontainer-guest",
            "-p",
            "wt-server",
        ),
        "build wt binaries",
    )?;
    runner.run(
        cmd!(
            "cargo",
            "build",
            "--quiet",
            "--release",
            "--target",
            MUSL_TARGET,
            "-p",
            "wt-agent-git",
            "--bin",
            "git-remote-ag",
            "--bin",
            "ag-git",
        ),
        "build static devcontainer binaries",
    )?;
    for name in [
        "wt-agent-git-gateway",
        "wt-agent-git-relay",
        "git-remote-ag",
        "ag-git",
        "wt",
        "wt-app-pane",
        "wt-app-info",
        "wt-app-proxy",
        "wt-server",
    ] {
        let source = release_binary(name);
        if DEVCONTAINER_BINARIES.contains(&name) {
            validate_static_binary(runner, &source, name)?;
        }
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

fn release_binary(name: &str) -> PathBuf {
    if DEVCONTAINER_BINARIES.contains(&name) {
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
    fn devcontainer_binaries_use_the_musl_release_directory() {
        assert_eq!(
            release_binary("ag-git"),
            Path::new("target/x86_64-unknown-linux-musl/release/ag-git")
        );
        assert_eq!(
            release_binary("git-remote-ag"),
            Path::new("target/x86_64-unknown-linux-musl/release/git-remote-ag")
        );
        assert_eq!(
            release_binary("wt-agent-git-relay"),
            Path::new("target/release/wt-agent-git-relay")
        );
    }

    #[test]
    fn devcontainer_binaries_must_be_static_and_free_of_glibc_versions() {
        validate_static_elf(
            "ag-git",
            "ELF program headers\n",
            "No version information\n",
        )
        .unwrap();

        insta::assert_snapshot!(
            validate_static_elf("ag-git", "  INTERP 0x000000\n", "").unwrap_err(),
            @"ag-git is dynamically linked: ELF program interpreter found"
        );
        insta::assert_snapshot!(
            validate_static_elf("ag-git", "", "Name: GLIBC_2.39\n").unwrap_err(),
            @"ag-git is dynamically linked: GLIBC symbol requirement found"
        );
    }
}
