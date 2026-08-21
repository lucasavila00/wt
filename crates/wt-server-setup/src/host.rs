use anyhow::{Context, Result};
use std::path::Path;
use wt_server::ServerConfig;
use wt_setup_core::Runner;

const SERVER_HOST_INSTALL: &[u8] = include_bytes!("../../../assets/server/install-host.sh");
pub(crate) const CODEX_AUTH_SHARE: &[u8] =
    include_bytes!("../../../assets/server/share-codex-auth.sh");

pub(crate) fn prepare_state(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let image_dir = config
        .image
        .devcontainer_path
        .parent()
        .context("image.devcontainer_path has no parent directory")?;
    let args = [
        "prepare".to_owned(),
        config.libvirt.network.clone(),
        image_dir.display().to_string(),
        config.install.binary_dir.display().to_string(),
        config.libvirt.worlds_dir.display().to_string(),
        config.registry_cache.state_dir.display().to_string(),
    ];
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run_script(CODEX_AUTH_SHARE, &[], "prepare Codex authentication share")?;
    runner.run_script(SERVER_HOST_INSTALL, &args, "prepare server host")
}

pub(crate) fn ensure_qemu_search_acl(runner: &impl Runner, path: &Path) -> Result<()> {
    runner.run_script(
        SERVER_HOST_INSTALL,
        &["acl", &path.display().to_string()],
        "ensure libvirt-qemu directory access",
    )
}
