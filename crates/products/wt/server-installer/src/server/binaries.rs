use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use wt_installer_support::cmd;
use wt_installer_support::{sudo_install, sudo_move, Runner};
use wt_server::ServerConfig;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";
pub(super) fn build(runner: &impl Runner) -> Result<()> {
    runner.run(
        cmd!(
            "cargo", "build", "--quiet", "--release", "-p", "wt-client",
            "--features", "host", "--bin", "wt",
        ),
        "build host wt",
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
            "wt-client",
            "--no-default-features",
            "--features",
            "guest",
            "--bin",
            "wt",
        ),
        "build static guest wt",
    )?;
    validate_static_binary(runner, &guest_binary(), "wt")?;
    Ok(())
}

pub(super) fn install(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let source = host_binary();
    let destination = config.install.binary_dir.join("wt");
    let temporary = config.install.binary_dir.join(".wt.wt-new");
    if temporary.exists() {
        bail!("stale binary install file exists: {}", temporary.display());
    }
    sudo_install(runner, &source, &temporary, 0o755)?;
    sudo_move(runner, &temporary, &destination)?;
    Ok(())
}

pub(crate) fn guest_binary() -> PathBuf {
    Path::new("target").join(MUSL_TARGET).join("release/wt")
}

fn host_binary() -> PathBuf {
    Path::new("target/release/wt").to_owned()
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
    fn runtime_boundaries_have_distinct_wt_binaries() {
        assert_eq!(guest_binary(), Path::new("target/x86_64-unknown-linux-musl/release/wt"));
        assert_eq!(host_binary(), Path::new("target/release/wt"));
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
