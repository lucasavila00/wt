use crate::runner::Runner;
use anyhow::{Context, Result};
use std::path::Path;
use wt_server::ServerConfig;

const SERVER_HOST_INSTALL: &[u8] = include_bytes!("../../../assets/install-server-host.sh");

pub(crate) fn prepare_state(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let image_dir = config
        .image
        .installed_path
        .parent()
        .context("image.installed_path has no parent directory")?;
    runner.run_script(
        SERVER_HOST_INSTALL,
        &[
            "prepare",
            &config.libvirt.network,
            &image_dir.display().to_string(),
            &config.install.binary_dir.display().to_string(),
            &config.libvirt.worlds_dir.display().to_string(),
            &config.registry_cache.state_dir.display().to_string(),
        ],
        "prepare server host",
    )
}

pub(crate) fn ensure_qemu_search_acl(runner: &impl Runner, path: &Path) -> Result<()> {
    runner.run_script(
        SERVER_HOST_INSTALL,
        &["acl", &path.display().to_string()],
        "ensure libvirt-qemu directory access",
    )
}
