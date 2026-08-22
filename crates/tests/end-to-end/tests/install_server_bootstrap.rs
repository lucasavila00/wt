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
#[ignore = "requires the Ubuntu 24.04 server build and libvirt toolchain"]
fn install_server_bootstraps_a_native_libvirt_installer() {
    let workspace = workspace();
    let install_script = std::fs::read_to_string(workspace.join("scripts/install-server"))
        .expect("read scripts/install-server");
    assert!(install_script.contains("setup_binary=target/release/wt-server-installer"));
    assert!(install_script
        .contains("scripts/cargo build --quiet --locked --release -p wt-server-installer"));
    assert!(!install_script.contains("--target x86_64-unknown-linux-musl -p wt-server-installer"));

    run(Command::new(workspace.join("scripts/cargo"))
        .current_dir(&workspace)
        .args([
            "build",
            "--quiet",
            "--locked",
            "--release",
            "-p",
            "wt-server-installer",
        ]));

    let installer = workspace.join("target/release/wt-server-installer");
    let dynamic = run(Command::new("readelf")
        .args(["--dynamic", "--wide"])
        .arg(&installer));
    assert!(
        String::from_utf8_lossy(&dynamic.stdout).contains("libvirt.so"),
        "native installer must link the host libvirt ABI"
    );

    run(Command::new(&installer).current_dir(&workspace).args([
        "validate",
        "--config",
        "examples/server-config/wt-server.development.toml",
    ]));
}
