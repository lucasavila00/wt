use super::fixture::run;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;
use wt_agent_tool_gateway::VSOCK_PORT;
use wt_end_to_end_tests::cmd;

pub(crate) fn unique_vsock_port() -> u32 {
    loop {
        let port = uuid::Uuid::new_v4().as_u128() as u32;
        if port > 1024 && port != VSOCK_PORT && port != u32::MAX {
            return port;
        }
    }
}

pub(crate) fn isolated_test_images(workspace: &Path, installed: &Path) -> TempDir {
    let images = tempfile::Builder::new()
        .prefix("wt-kvm-images-")
        .tempdir_in("/var/tmp")
        .unwrap();
    fs::set_permissions(images.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let retained = images.path().join("retained.qcow2");
    run(
        cmd!("qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", installed, &retained,),
        "create isolated KVM test image",
    );
    fs::set_permissions(&retained, fs::Permissions::from_mode(0o644)).unwrap();
    let installed_manifest = format!("{}.manifest.json", installed.display());
    let isolated_manifest = format!("{}.manifest.json", retained.display());
    fs::copy(installed_manifest, isolated_manifest).unwrap();
    let prepare = workspace.join("assets/world/host/prepare.sh");
    run(
        cmd!(
            "sudo",
            "-n",
            "virt-customize",
            "--no-network",
            "-a",
            &retained,
            "--upload",
            format!("{}:/usr/local/libexec/wt-host-prepare", prepare.display()),
            "--chmod",
            "0755:/usr/local/libexec/wt-host-prepare",
        ),
        "install current host prepare asset in isolated test image",
    );
    fs::set_permissions(&retained, fs::Permissions::from_mode(0o644)).unwrap();
    images
}
