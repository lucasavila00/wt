use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("end-to-end crate must be inside the workspace")
        .to_owned()
}

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("run installer bootstrap command");
    assert!(
        output.status.success(),
        "command failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

#[test]
fn install_server_script_is_complete() {
    let install_script = std::fs::read_to_string(workspace().join("scripts/install-server"))
        .expect("read scripts/install-server");

    insta::assert_snapshot!(install_script);
}

#[test]
#[ignore = "requires the Ubuntu 24.04 server build and libvirt toolchain"]
fn install_server_bootstraps_a_native_libvirt_installer() {
    let workspace = workspace();

    run(Command::new("cargo").current_dir(&workspace).args([
        "build",
        "--quiet",
        "--locked",
        "--release",
        "-p",
        "wt-server-installer",
        "--bin",
        "wts",
    ]));

    let installer = workspace.join("target/release/wts");
    let dynamic = run(Command::new("readelf")
        .args(["--dynamic", "--wide"])
        .arg(&installer));
    assert!(
        String::from_utf8_lossy(&dynamic.stdout).contains("libvirt.so"),
        "native installer must link the host libvirt ABI"
    );
}
