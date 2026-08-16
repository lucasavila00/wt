use super::fixture::run;
use std::fs;
use std::path::Path;
use wt_command::cmd;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

pub(crate) fn prepare_test_binaries(workspace: &Path, destination: &Path) {
    let mut build = cmd!(
        env!("CARGO"),
        "build",
        "--target",
        MUSL_TARGET,
        "-p",
        "wt-agent-git",
        "-p",
        "wt-devcontainer-guest",
    );
    build.current_dir(workspace);
    run(build, "build static KVM test binaries");

    fs::create_dir(destination).unwrap();
    let binaries = workspace.join("target").join(MUSL_TARGET).join("debug");
    for name in [
        "wt-agent-git-gateway",
        "wt-agent-git-relay",
        "wt-app-pane",
        "wt-app-info",
        "wt-app-proxy",
        "git-remote-ag",
        "ag-git",
    ] {
        let source = binaries.join(name);
        assert_static(&source, name);
        fs::copy(source, destination.join(name)).unwrap();
    }
}

fn assert_static(path: &Path, name: &str) {
    let headers = std::process::Command::new("readelf")
        .args(["--program-headers", "--wide"])
        .arg(path)
        .output()
        .unwrap();
    assert!(headers.status.success(), "readelf failed for {name}");
    assert!(
        !String::from_utf8_lossy(&headers.stdout).contains("INTERP"),
        "{name} has a dynamic program interpreter"
    );

    let versions = std::process::Command::new("readelf")
        .args(["--version-info", "--wide"])
        .arg(path)
        .output()
        .unwrap();
    assert!(versions.status.success(), "readelf failed for {name}");
    assert!(
        !String::from_utf8_lossy(&versions.stdout).contains("GLIBC_"),
        "{name} requires GLIBC"
    );
}
