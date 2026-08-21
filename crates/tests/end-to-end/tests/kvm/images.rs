use super::fixture::run;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;
use wt_agent_git_gateway::VSOCK_PORT;
use wt_end_to_end_tests::cmd;

pub(crate) fn unique_vsock_port() -> u32 {
    loop {
        let port = uuid::Uuid::new_v4().as_u128() as u32;
        if port > 1024 && port != VSOCK_PORT && port != u32::MAX {
            return port;
        }
    }
}

pub(crate) fn isolated_test_images(
    workspace: &Path,
    installed_devcontainer: &Path,
    installed_host: &Path,
) -> TempDir {
    let images = tempfile::Builder::new()
        .prefix("wt-kvm-images-")
        .tempdir_in("/var/tmp")
        .unwrap();
    fs::set_permissions(images.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let devcontainer = images.path().join("devcontainer.qcow2");
    let host = images.path().join("host.qcow2");
    for (installed, isolated) in [
        (installed_devcontainer, devcontainer.as_path()),
        (installed_host, host.as_path()),
    ] {
        run(
            cmd!(
                "qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", installed,
                isolated,
            ),
            "create isolated KVM test image",
        );
        fs::set_permissions(isolated, fs::Permissions::from_mode(0o644)).unwrap();
        let installed_manifest = format!("{}.manifest.json", installed.display());
        let isolated_manifest = format!("{}.manifest.json", isolated.display());
        fs::copy(installed_manifest, isolated_manifest).unwrap();
    }
    let prepare = workspace.join("assets/world/host/prepare.sh");
    run(
        cmd!(
            "sudo",
            "-n",
            "virt-customize",
            "--no-network",
            "-a",
            &host,
            "--upload",
            format!("{}:/usr/local/libexec/wt-host-prepare", prepare.display()),
            "--chmod",
            "0755:/usr/local/libexec/wt-host-prepare",
        ),
        "install current host prepare asset in isolated test image",
    );
    fs::set_permissions(&host, fs::Permissions::from_mode(0o644)).unwrap();
    images
}
