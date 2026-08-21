use super::*;

pub(crate) fn assert_terminal_stack(harness: &KvmHarness, name: &InstanceName) {
    let output = guest_command(
        harness,
        name,
        r#"set -eu
tmux -V
dpkg-query -W -f='${Version}\n' byobu
tmux show-options -gvw history-limit
tmux show-options -gv -t wt-host default-terminal
TERM=ghostty tput colors
TERM=xterm-ghostty tput colors"#,
    )
    .output()
    .unwrap();
    ensure_success("inspect terminal stack", &output).unwrap();
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
tmux 3.6b
7.15-0ubuntu1
100000
tmux-256color
256
256
"###);
}
