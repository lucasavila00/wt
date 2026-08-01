use super::*;

pub(super) fn verify_source_guest(
    harness: &KvmHarness,
    name: &InstanceName,
    agent: &SshAgent,
    timings: &mut Timings,
) -> Result<(), String> {
    let temp = &harness.temp;
    let git = &harness.git;
    let server_config_path = harness.server_config_path.clone();
    let Response::Instances { instances } =
        call_api(temp.path(), &server_config_path, Operation::List)
    else {
        return Err("expected list response".to_owned());
    };
    assert_eq!(instances.len(), 1);
    timings.run("sync SSH inventory", || sync_inventory(&instances))?;

    let host_alias = format!("local.{}-host", name.as_str());
    let vs_alias = format!("local.{}-vs", name.as_str());
    let ssh_config = temp.path().join(".ssh/config");
    let output = timings.run("verify guest SSH", || {
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "test -d /workspace/.git && test ! -e /etc/sudoers.d/wt-setup && test ! -e /var/lib/wt-setup/source && test ! -e /var/lib/wt-setup/git-known-hosts && test ! -e /var/lib/wt-setup/authorized-keys && test ! -e /var/lib/wt-setup/deferred-packages && test ! -e /var/lib/wt-setup/root-prepared && printf 'BACKGROUND=k\\nFOREGROUND=w\\nMONOCHROME=1\\n' | cmp - /home/wt/.byobu/color && test \"$(stat -c '%U:%G:%a' /home/wt/.byobu/color)\" = wt:wt:644 && test \"$(TERM=ghostty tput colors)\" = 256 && test \"$(TERM=xterm-ghostty tput colors)\" = 256 && test \"$(nproc)\" = 1 && memory=$(awk '/MemTotal/ {print $2}' /proc/meminfo) && test \"$memory\" -ge 800000 && test \"$memory\" -le 1100000 && sectors=$(cat /sys/block/vda/size) && test \"$sectors\" -ge 67108864",
        )
        .output()
        .map_err(|error| error.to_string())
    })?;
    ensure_success("enter fixture guest host", &output)?;
    let output = timings.run("verify direct devcontainer SSH", || {
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            &vs_alias,
            "test -d /workspaces/small-devcontainer-fixture && ssh-add -L >/dev/null",
        )
        .env("SSH_AUTH_SOCK", &agent.socket)
        .output()
        .map_err(|error| error.to_string())
    })?;
    ensure_success("enter fixture devcontainer over SSH", &output)?;
    let executable = "/usr/bin/byobu-tmux";
    let output = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        &host_alias,
        format!("test -x {executable}"),
    )
    .output()
    .map_err(|error| error.to_string())?;
    ensure_success("verify Byobu frontend", &output)?;
    let byobu_version = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "dpkg-query",
            "-W",
            r"-f=\${Version}",
            "byobu",
        ),
        "read Byobu package version",
    );
    if byobu_version != "7.15-0ubuntu1" {
        return Err(format!(
            "unexpected Byobu version: {byobu_version:?}; expected 7.15-0ubuntu1"
        ));
    }
    let tmux_version = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/byobu-tmux",
            "-V",
        ),
        "read Byobu tmux version",
    );
    if tmux_version.trim() != "tmux 3.6b" {
        return Err(format!(
            "unexpected Byobu tmux version: {tmux_version:?}; expected tmux 3.6b"
        ));
    }
    let mut persistent = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        name.as_str(),
    )
    .env("SSH_AUTH_SOCK", &agent.socket)
    .env("TERM", "xterm-ghostty")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .map_err(|error| format!("start persistent app shell: {error}"))?;
    persistent
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            b"ssh-add -L >/dev/null && export WT_PERSISTENCE_MARKER=retained; cd /tmp; printf '%s\\n' \"$WT_PERSISTENCE_MARKER:$PWD:$TERM\"\n",
        )
        .map_err(|error| format!("initialize persistent app shell: {error}"))?;
    wait_for_line(&mut persistent, "retained:/tmp:tmux-256color")?;
    disconnect(&mut persistent, "initial persistent app shell")?;

    let mut reattached = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        name.as_str(),
    )
    .env("SSH_AUTH_SOCK", &agent.socket)
    .env("TERM", "xterm-ghostty")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .map_err(|error| format!("reattach persistent app shell: {error}"))?;
    reattached
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            b"test \"$WT_PERSISTENCE_MARKER\" = retained && test \"$PWD\" = /tmp && printf 'persistence-%s\\n' \"$WT_PERSISTENCE_MARKER\"\n",
        )
        .map_err(|error| format!("verify persistent app shell: {error}"))?;
    wait_for_line(&mut reattached, "persistence-retained")?;
    disconnect(&mut reattached, "reattached app shell")?;

    let output = cmd!(
        "ssh",
        "-F",
        &ssh_config,
        "-i",
        &git.guest_key,
        &host_alias,
        "/usr/bin/byobu-tmux",
        "source-file",
        "/usr/share/byobu/profiles/tmuxrc",
        "\\;",
        "new-window",
        "\\;",
        "split-window",
        "\\;",
        "list-panes",
        "-a",
        "-F",
        "'#{pane_start_command}'",
    )
    .output()
    .map_err(|error| error.to_string())?;
    ensure_success(
        "reload Byobu profile and create persistent app window and split",
        &output,
    )?;
    let panes = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let mut panes = panes
        .lines()
        .filter(|pane| *pane != "byobu-janitor")
        .collect::<Vec<_>>();
    panes.sort_unstable();
    if panes
        != [
            "/usr/local/bin/wt-app-pane",
            "/usr/local/bin/wt-app-pane",
            "/usr/local/bin/wt-setup-world",
        ]
    {
        return Err(format!("unexpected tmux pane commands: {panes:?}"));
    }
    let prefix = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-gv",
            "prefix",
        ),
        "read persistent session prefix",
    );
    let expected_prefix = "F12";
    if prefix.trim() != expected_prefix {
        return Err(format!(
            "unexpected Byobu session prefix: {prefix:?}; expected {expected_prefix}"
        ));
    }
    let remain_on_exit = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-gv",
            "remain-on-exit",
        ),
        "read persistent session remain-on-exit",
    );
    if remain_on_exit.trim() != "off" {
        return Err(format!(
            "unexpected remain-on-exit after setup: {remain_on_exit:?}; expected off"
        ));
    }
    let focus_events = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-gv",
            "focus-events",
        ),
        "read persistent session focus-events",
    );
    if focus_events.trim() != "on" {
        return Err(format!(
            "unexpected focus-events value: {focus_events:?}; expected on"
        ));
    }
    let default_terminal = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-gv",
            "default-terminal",
        ),
        "read persistent session default-terminal",
    );
    if default_terminal.trim() != "tmux-256color" {
        return Err(format!(
            "unexpected default-terminal value: {default_terminal:?}; expected tmux-256color"
        ));
    }
    let mouse = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-gv",
            "mouse",
        ),
        "read persistent session mouse setting",
    );
    if mouse.trim() != "on" {
        return Err(format!("unexpected mouse value: {mouse:?}; expected on"));
    }
    let terminal_features = git_output(
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "show-options",
            "-sv",
            "terminal-features",
        ),
        "read persistent session terminal features",
    );
    if !terminal_features
        .lines()
        .any(|line| line == "xterm-ghostty:clipboard:hyperlinks")
    {
        return Err(format!(
            "terminal features do not include Ghostty clipboard and hyperlinks: {terminal_features:?}"
        ));
    }
    for (key, expected) in [
        (
            "WheelUpPane",
            "bind-key -T root WheelUpPane if-shell -F -t = \"#{mouse_any_flag}\" \"send-keys -M\" \"copy-mode -e -t =\"",
        ),
        (
            "WheelDownPane",
            "bind-key -T root WheelDownPane if-shell -F -t = \"#{mouse_any_flag}\" \"send-keys -M\" \"select-pane -t =\"",
        ),
    ] {
        let binding = git_output(
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &host_alias,
                "/usr/bin/tmux",
                "list-keys",
                "-T",
                "root",
                key,
            ),
            "read persistent session mouse wheel binding",
        );
        if binding.trim() != expected {
            return Err(format!(
                "unexpected {key} binding: {binding:?}; expected {expected:?}"
            ));
        }
    }

    let branch = format!("wt-e2e-{}", std::process::id());
    let app_commands = temp.path().join("app-commands");
    fs::write(
        &app_commands,
        format!(
            "set -eu\ntest -n \"$BASH_VERSION\"\ntest \"$(id -u)\" -eq 0\ntest \"$(pwd)\" = /workspaces/small-devcontainer-fixture\ntest \"$(git config user.name)\" = 'WT E2E'\ntest \"$(git config user.email)\" = wt@example.invalid\ngit switch -c {branch}\nprintf 'committed\\n' > wt-e2e.txt\ngit add wt-e2e.txt\ngit commit -m wt-e2e\n"
        ),
    )
    .map_err(|error| error.to_string())?;
    let input = fs::File::open(&app_commands).map_err(|error| error.to_string())?;
    let output = timings.run("commit from app container", || {
        cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &vs_alias,
            "cd /workspaces/small-devcontainer-fixture && exec /bin/bash",
        )
        .stdin(Stdio::from(input))
        .output()
        .map_err(|error| error.to_string())
    })?;
    ensure_success("commit from fixture app container", &output)?;
    Ok(())
}
