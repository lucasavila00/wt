use anyhow::{Context, Result};
use std::path::Path;
use wt_installer_support::Runner;
use wt_server::ServerConfig;

const WT_IDENTITY_CONTRACT: &[u8] = include_bytes!("../../../../../assets/server/wt-identity.sh");
const SERVER_HOST_INSTALL_FLOW: &[u8] =
    include_bytes!("../../../../../assets/server/install-host.sh");
const CODEX_AUTH_SHARE_FLOW: &[u8] =
    include_bytes!("../../../../../assets/server/share-codex-auth.sh");
const SSH_AUTHORIZED_KEYS_SHARE_FLOW: &[u8] =
    include_bytes!("../../../../../assets/server/share-ssh-authorized-keys.sh");
const PUBLISH_SHARED_FILE: &[u8] =
    include_bytes!("../../../../../assets/server/publish-shared-file.sh");

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
    shell_with_shared_file_publisher(CODEX_AUTH_SHARE_FLOW)
}

pub(crate) fn ssh_authorized_keys_share() -> Vec<u8> {
    shell_with_shared_file_publisher(SSH_AUTHORIZED_KEYS_SHARE_FLOW)
}

fn shell_with_shared_file_publisher(flow: &[u8]) -> Vec<u8> {
    const SHEBANG: &[u8] = b"#!/bin/sh\n";
    let body = flow
        .strip_prefix(SHEBANG)
        .expect("WT server shell asset must use #!/bin/sh");
    let mut script = Vec::with_capacity(
        SHEBANG.len() + WT_IDENTITY_CONTRACT.len() + PUBLISH_SHARED_FILE.len() + body.len() + 2,
    );
    script.extend_from_slice(SHEBANG);
    script.extend_from_slice(WT_IDENTITY_CONTRACT);
    script.push(b'\n');
    script.extend_from_slice(PUBLISH_SHARED_FILE);
    script.push(b'\n');
    script.extend_from_slice(body);
    script
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
        &shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
        &["check", args[1], args[2], args[3], args[4]],
        "validate server host",
    )?;
    runner.run_script(
        &codex_auth_share(),
        &["--check"],
        "validate Codex authentication share",
    )?;
    runner.run_script(
        &ssh_authorized_keys_share(),
        &["--check"],
        "validate SSH authorized keys share",
    )?;
    runner.run_script(
        &shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
        &args,
        "prepare server host",
    )?;
    runner.run_script(
        &codex_auth_share(),
        &[],
        "prepare Codex authentication share",
    )?;
    runner.run_script(
        &ssh_authorized_keys_share(),
        &[],
        "prepare SSH authorized keys share",
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
    use std::cell::RefCell;

    struct RecordingRunner(RefCell<Vec<(String, Vec<String>)>>);

    impl Runner for RecordingRunner {
        fn output(&self, _command: std::process::Command) -> Result<std::process::Output> {
            panic!("prepare_state must use run_script")
        }

        fn run_script(&self, _script: &[u8], args: &[&str], action: &str) -> Result<()> {
            self.0.borrow_mut().push((
                action.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ));
            Ok(())
        }
    }

    fn config() -> ServerConfig {
        ServerConfig {
            version: 1,
            test_server: true,
            image: wt_server::ImageConfig {
                path: "/var/lib/wt/images/retained.qcow2".into(),
            },
            libvirt: wt_server::ServerLibvirtConfig {
                network: "default".to_owned(),
                worlds_dir: "/var/lib/libvirt/images/wt".into(),
            },
            agent_tools: wt_server::AgentToolsConfig {
                vsock_port: wt_server::DEFAULT_AGENT_TOOL_VSOCK_PORT,
                github: None,
                gitlab: None,
            },
            guest: wt_server::GuestConfig {
                boot_timeout_seconds: 300,
                readiness_timeout_seconds: 900,
            },
            install: wt_server::InstallConfig {
                binary_dir: "/usr/local/bin".into(),
            },
        }
    }

    #[test]
    fn composed_shell_assets_keep_their_interpreter() {
        for script in [
            shell_with_identity_contract(SERVER_HOST_INSTALL_FLOW),
            codex_auth_share(),
            ssh_authorized_keys_share(),
        ] {
            assert!(script.starts_with(
                b"#!/bin/sh\n# shellcheck shell=sh\n# Canonical WT host/guest filesystem identity."
            ));
        }
        for script in [codex_auth_share(), ssh_authorized_keys_share()] {
            assert!(script
                .windows(b"wt_publish_shared_file()".len())
                .any(|window| { window == b"wt_publish_shared_file()" }));
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

    #[test]
    fn host_and_auth_preconditions_run_before_mutation() {
        let runner = RecordingRunner(RefCell::new(Vec::new()));
        prepare_state(&runner, &config()).unwrap();

        assert_eq!(
            runner.0.into_inner(),
            vec![
                (
                    "validate server host".to_owned(),
                    vec![
                        "check".to_owned(),
                        "default".to_owned(),
                        "/var/lib/wt/images".to_owned(),
                        "/usr/local/bin".to_owned(),
                        "/var/lib/libvirt/images/wt".to_owned(),
                    ],
                ),
                (
                    "validate Codex authentication share".to_owned(),
                    vec!["--check".to_owned()],
                ),
                (
                    "validate SSH authorized keys share".to_owned(),
                    vec!["--check".to_owned()],
                ),
                (
                    "prepare server host".to_owned(),
                    vec![
                        "prepare".to_owned(),
                        "default".to_owned(),
                        "/var/lib/wt/images".to_owned(),
                        "/usr/local/bin".to_owned(),
                        "/var/lib/libvirt/images/wt".to_owned(),
                    ],
                ),
                ("prepare Codex authentication share".to_owned(), vec![]),
                ("prepare SSH authorized keys share".to_owned(), vec![]),
            ]
        );
    }

    #[test]
    fn bootstrap_validates_managed_paths_before_creating_the_account() {
        let bootstrap = include_str!("../../../../../scripts/bootstrap-server-user");
        let first_account_mutation = bootstrap.find("groupadd --gid").unwrap();
        for precondition in [
            "WT SSH path must be a regular directory",
            "authorized keys conflict",
            "sudoers conflict",
        ] {
            assert!(bootstrap.find(precondition).unwrap() < first_account_mutation);
        }
    }
}
