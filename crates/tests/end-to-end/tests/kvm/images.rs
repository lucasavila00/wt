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

pub(crate) fn isolated_test_images(
    workspace: &Path,
    installed: &Path,
    binary_dir: &Path,
) -> TempDir {
    let images = tempfile::Builder::new()
        .prefix("wt-kvm-images-")
        .tempdir_in("/var/tmp")
        .unwrap();
    fs::set_permissions(images.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let guest_image = images.path().join("guest.qcow2");
    run(
        cmd!(
            "qemu-img",
            "create",
            "-q",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            installed,
            &guest_image,
        ),
        "create isolated KVM test image",
    );
    fs::set_permissions(&guest_image, fs::Permissions::from_mode(0o644)).unwrap();
    let installed_manifest = format!("{}.manifest.json", installed.display());
    let isolated_manifest = format!("{}.manifest.json", guest_image.display());
    fs::copy(installed_manifest, isolated_manifest).unwrap();
    let inputs = [
        (
            workspace.join("assets/world/guest/prepare.sh"),
            "/usr/local/libexec/wt-guest-prepare",
        ),
        (
            workspace.join("assets/world/shared/install-agent-tools.sh"),
            "/usr/local/libexec/wt-guest-agent-tools",
        ),
        (binary_dir.join("wtg"), "/usr/local/bin/wtg"),
    ];
    let mut customize = cmd!(
        "sudo",
        "-n",
        "virt-customize",
        "--no-network",
        "-a",
        &guest_image
    );
    for (source, guest_path) in &inputs {
        customize
            .arg("--upload")
            .arg(format!("{}:{guest_path}", source.display()));
    }
    for (_, guest_path) in &inputs {
        customize.arg("--chmod").arg(format!("0755:{guest_path}"));
    }
    run(
        customize,
        "install current guest assets in isolated test image",
    );
    fs::set_permissions(&guest_image, fs::Permissions::from_mode(0o644)).unwrap();
    images
}
