use super::fixture::run;
use std::fs;
use std::path::Path;
use wt_command::cmd;

const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

pub(crate) fn prepare_test_binaries(workspace: &Path, destination: &Path) {
    let mut native = cmd!(env!("CARGO"), "build", "-p", "wt-devcontainer-guest",);
    native.current_dir(workspace);
    run(native, "build native KVM test binaries");

    let mut static_agent_git = cmd!(
        env!("CARGO"),
        "build",
        "--target",
        MUSL_TARGET,
        "-p",
        "wt-agent-git",
    );
    static_agent_git.current_dir(workspace);
    run(static_agent_git, "build static KVM agent Git binaries");

    fs::create_dir(destination).unwrap();
    let native = workspace.join("target/debug");
    let static_agent_git = workspace.join("target").join(MUSL_TARGET).join("debug");
    for (source, name) in [
        (
            static_agent_git.join("wt-agent-git-gateway"),
            "wt-agent-git-gateway",
        ),
        (
            static_agent_git.join("wt-agent-git-relay"),
            "wt-agent-git-relay",
        ),
        (native.join("wt-app-pane"), "wt-app-pane"),
        (native.join("wt-app-info"), "wt-app-info"),
        (native.join("wt-app-proxy"), "wt-app-proxy"),
        (static_agent_git.join("git-remote-ag"), "git-remote-ag"),
        (static_agent_git.join("ag-git"), "ag-git"),
    ] {
        fs::copy(source, destination.join(name)).unwrap();
    }
}
