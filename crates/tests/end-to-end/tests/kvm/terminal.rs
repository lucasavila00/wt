use super::*;

pub(crate) fn assert_shared_terminal_stack(
    harness: &KvmHarness,
    devcontainer: &InstanceName,
    host: &InstanceName,
) {
    let devcontainer = terminal_signature(guest_command(
        harness,
        devcontainer,
        &terminal_signature_command("wt-app"),
    ));
    let host = terminal_signature(host_command(
        harness,
        host,
        &terminal_signature_command("wt-host"),
    ));

    assert_eq!(
        devcontainer, host,
        "world kinds use different terminal stacks"
    );
    insta::assert_snapshot!(devcontainer, @r###"
tmux 3.6b
7.15-0ubuntu1
100000
tmux-256color
on
on
on
on
256
256
wt:wt 644
BACKGROUND=k
FOREGROUND=w
MONOCHROME=1
"###);
}

fn terminal_signature(mut command: std::process::Command) -> String {
    command.env_remove("SSH_AUTH_SOCK");
    let output = command.output().unwrap();
    ensure_success("inspect shared terminal stack", &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn terminal_signature_command(session: &str) -> String {
    format!(
        r#"set -eu
tmux -V
dpkg-query -W -f='${{Version}}\n' byobu
tmux show-options -gvw history-limit
tmux show-options -gv -t {session} default-terminal
tmux show-options -gv -t {session} mouse
tmux show-options -sv set-clipboard
tmux show-options -gvw allow-passthrough
tmux show-options -gv -t {session} focus-events
TERM=ghostty tput colors
TERM=xterm-ghostty tput colors
stat -c '%U:%G %a' /home/wt/.byobu/color
cat /home/wt/.byobu/color"#
    )
}
