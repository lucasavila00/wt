use super::fixture::run;
use std::fs;
use std::path::Path;
use wt_end_to_end_tests::cmd;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

pub(crate) fn prepare_test_binaries(workspace: &Path, destination: &Path) {
    let mut build_client = cmd!(env!("CARGO"), "build", "-p", "wt-client", "--bin", "wt");
    build_client.current_dir(workspace);
    run(build_client, "build current WT client for KVM tests");

    let mut build = cmd!(
        env!("CARGO"),
        "build",
        "--target",
        MUSL_TARGET,
        "-p",
        "wt-agent-tool-gateway",
        "--bin",
        "wt-agent-tool-gateway",
    );
    build.current_dir(workspace);
    run(build, "build static KVM test binaries");
    let mut build_guest = cmd!(
        env!("CARGO"),
        "build",
        "--target",
        MUSL_TARGET,
        "-p",
        "wt-client",
        "--no-default-features",
        "--features",
        "guest",
        "--bin",
        "wtg",
    );
    build_guest.current_dir(workspace);
    run(build_guest, "build static KVM guest runtime");

    fs::create_dir(destination).unwrap();
    fs::copy(workspace.join("target/debug/wt"), destination.join("wt")).unwrap();
    let binaries = workspace.join("target").join(MUSL_TARGET).join("debug");
    for name in ["wt-agent-tool-gateway", "wtg"] {
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
