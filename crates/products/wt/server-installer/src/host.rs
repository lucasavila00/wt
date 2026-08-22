use anyhow::{Context, Result};
use std::path::Path;
use wt_installer_support::Runner;
use wt_server::ServerConfig;

const WT_IDENTITY_CONTRACT: &[u8] = include_bytes!("../../../../../assets/server/wt-identity.sh");
const SERVER_HOST_INSTALL_FLOW: &[u8] =
    include_bytes!("../../../../../assets/server/install-host.sh");
const CODEX_AUTH_SHARE_FLOW: &[u8] =
    include_bytes!("../../../../../assets/server/share-codex-auth.sh");

fn shell_with_identity_contract(flow: &[u8]) -> Vec<u8> {
    const SHEBANG: &[u8] = b"#!/bin/sh\n";
    let body = flow
        .strip_prefix(SHEBANG)
        .expect("WT server shell asset must use #!/bin/sh");
    let mut script =
        Vec::with_capacity(SHEBANG.len() + WT_IDENTITY_CONTRACT.len() + body.len() + 1);
    script.extend_from_slice(SHEBANG);
    script.extend_from_slice(WT_IDENTITY_CONTRACT);
    script.push(b'\n');
    script.extend_from_slice(body);
    script
}

pub(crate) fn codex_auth_share() -> Vec<u8> {
    shell_with_identity_contract(CODEX_AUTH_SHARE_FLOW)
}

pub(crate) fn prepare_state(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let image_dir = config
        .image
        .path
        .parent()
        .context("image.path has no parent directory")?;
    let args = [
        "prepare".to_owned(),
        config.libvirt.network.clone(),
        image_dir.display().to_string(),
        config.install.binary_dir.display().to_string(),
        config.libvirt.worlds_dir.display().to_string(),
    ];
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run_script(
        &codex_auth_share(),
        &[],
        "prepare Codex authentication share",
    )?;
    runner.run_script(
        &shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
        &args,
        "prepare server host",
    )
}

pub(crate) fn ensure_qemu_search_acl(runner: &impl Runner, path: &Path) -> Result<()> {
    runner.run_script(
        &shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
        &["acl", &path.display().to_string()],
        "ensure libvirt-qemu directory access",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_shell_assets_keep_their_interpreter() {
        for script in [
            shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
            codex_auth_share(),
        ] {
            assert!(script.starts_with(
                b"#!/bin/sh\n# shellcheck shell=sh\n# Canonical WT host/guest filesystem identity."
            ));
        }
    }

    #[test]
    fn host_directory_policies_keep_worlds_and_codex_distinct() {
        let flow = std::str::from_utf8(SERVER_HOST_INSTALL_FLOW).unwrap();
        assert!(flow
            .contains("ensure_directory \"$WT_IDENTITY_UID\" \"$kvm_gid\" 2770 \"$worlds_dir\""));
        assert!(flow.contains("ensure_qemu_acl \"$worlds_dir\""));
        assert!(flow.contains("wt_require_owned_directory \"$WT_IDENTITY_HOME\""));
        assert!(flow.contains("wt_require_owned_directory \"$WT_IDENTITY_HOME/.codex\""));
        assert!(flow.contains(
            "ensure_directory \"$WT_IDENTITY_UID\" \"$WT_IDENTITY_GID\" 700 \"$WT_IDENTITY_HOME/.codex/sessions\""
        ));
    }
}
