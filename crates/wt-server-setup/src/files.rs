use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Group, User};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(crate) fn require_root_file(path: &Path, mode: u32) -> Result<()> {
    require_named_file(path, "root", "root", mode)
}

pub(crate) fn require_named_file(path: &Path, owner: &str, group: &str, mode: u32) -> Result<()> {
    let uid = User::from_name(owner)
        .with_context(|| format!("look up user {owner}"))?
        .with_context(|| format!("required user does not exist: {owner}"))?
        .uid;
    let gid = Group::from_name(group)
        .with_context(|| format!("look up group {group}"))?
        .with_context(|| format!("required group does not exist: {group}"))?
        .gid;
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if !metadata.is_file()
        || metadata.uid() != uid.as_raw()
        || metadata.gid() != gid.as_raw()
        || metadata.mode() & 0o7777 != mode
    {
        bail!(
            "file drift at {}: expected {owner}:{group}, mode={mode:04o}",
            path.display(),
        );
    }
    Ok(())
}

pub(crate) fn sudo_install(
    runner: &impl Runner,
    source: &Path,
    destination: &Path,
    mode: u32,
) -> Result<()> {
    sudo_install_owned(runner, source, destination, "root", "root", mode)
}

pub(crate) fn sudo_install_owned(
    runner: &impl Runner,
    source: &Path,
    destination: &Path,
    owner: &str,
    group: &str,
    mode: u32,
) -> Result<()> {
    runner.run(
        "sudo",
        &[
            "install".into(),
            "-o".into(),
            owner.into(),
            "-g".into(),
            group.into(),
            "-m".into(),
            format!("{mode:04o}").into(),
            source.as_os_str().to_owned(),
            destination.as_os_str().to_owned(),
        ],
        "install file",
    )
}

pub(crate) fn sudo_move(runner: &impl Runner, source: &Path, destination: &Path) -> Result<()> {
    runner.run(
        "sudo",
        &[
            "mv".into(),
            "--".into(),
            source.as_os_str().to_owned(),
            destination.as_os_str().to_owned(),
        ],
        "publish installed file",
    )
}
