use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use wt_command::cmd;

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn test_home(contexts: &str, helper: &str) -> (tempfile::TempDir, std::ffi::OsString) {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        format!("version = 1\n{contexts}"),
    )
    .unwrap();
    write_executable(&bin.join("wt-server"), helper);
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    (temp, path)
}

#[test]
fn forwards_opaque_arguments_and_server_output() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
set -eu
IFS= read -r start
printf '%s\n' "$start" > "$HOME/start.json"
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"output","stream":"stdout","text":"from server\n"}'
printf '%s\n' '{"message":"output","stream":"stderr","text":"warning\n"}'
printf '%s\n' '{"message":"exit","code":7}'
"#,
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "future-command", "--new-flag")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"from server\n");
    assert_eq!(output.stderr, b"warning\n");
    let start: Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("start.json")).unwrap()).unwrap();
    assert_eq!(
        start,
        serde_json::json!({
            "message": "start",
            "schema": 1,
            "context": "ars",
            "args": ["future-command", "--new-flag"]
        })
    );
}

#[test]
fn multiple_contexts_require_an_explicit_selection() {
    let contexts = "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n[[contexts]]\nname = \"lab\"\nkind = \"bare_metal_local\"\n";
    let (temp, path) = test_home(contexts, "#!/bin/sh\ntouch \"$HOME/contacted\"\nexit 2\n");
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ls")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!temp.path().join("contacted").exists());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"wt: multiple contexts are configured; use `wt --ctx NAME COMMAND`");
}

#[test]
fn inventory_effect_updates_only_the_selected_context() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n[[contexts]]\nname = \"lab\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
set -eu
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"effect","id":1,"effect":"replace_ssh_inventory","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"jsdev","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"jsdev/"}]}'
IFS= read -r result
printf '%s\n' "$result" > "$HOME/effect-result.json"
printf '%s\n' '{"message":"exit","code":0}'
"#,
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "--ctx", "ars", "sync")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = fs::read_to_string(temp.path().join(".ssh/wt/contexts/ars/config")).unwrap();
    assert!(config.contains("Host ars.jsdev\n"));
    assert!(!config.contains("Host jsdev\n"));
    assert!(!temp.path().join(".ssh/wt/contexts/lab/config").exists());
    let result: Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("effect-result.json")).unwrap())
            .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "message": "effect_result",
            "id": 1,
            "outcome": "ok",
            "output": "none"
        })
    );
}

#[test]
fn reports_schema_mismatch() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
IFS= read -r start
printf '%s\n' '{"message":"schema_mismatch","client_schema":1,"server_schema":2}'
"#,
    );
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ls")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"wt: client schema 1 does not match server schema 2; upgrade the wt client");
}

#[test]
fn forwards_requested_standard_input() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
set -eu
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"read_input","id":8}'
IFS= read -r input
printf '%s\n' "$input" > "$HOME/input.json"
printf '%s\n' '{"message":"exit","code":0}'
"#,
    );
    let mut child = cmd!(env!("CARGO_BIN_EXE_wt"), "new")
        .env("HOME", temp.path())
        .env("PATH", path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"answer\n").unwrap();
    assert!(child.wait().unwrap().success());
    let input: Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input,
        serde_json::json!({
            "message": "input",
            "id": 8,
            "text": "answer\n",
            "eof": false
        })
    );
}

#[test]
fn exec_ssh_effect_replaces_the_client_process() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
set -eu
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"effect","id":1,"effect":"exec_ssh","target":"ars.world"}'
IFS= read -r result
exit 2
"#,
    );
    write_executable(
        &temp.path().join("bin/ssh"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/ssh-args\"\nexit 23\n",
    );
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "new")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-args")).unwrap(),
        "--\nars.world\n"
    );
}

#[test]
fn unknown_effect_is_not_executed() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"effect","id":1,"effect":"run_shell","command":"touch should-not-exist"}'
"#,
    );
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ls")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!temp.path().join("should-not-exist").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown variant `run_shell`"));
}

#[test]
fn launch_code_effect_uses_the_qualified_alias() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
        r#"#!/bin/sh
set -eu
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"effect","id":1,"effect":"launch_code","target":"ars.world"}'
IFS= read -r result
printf '%s\n' '{"message":"exit","code":0}'
"#,
    );
    write_executable(
        &temp.path().join("bin/ssh"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/ssh-args\"\nprintf '%s\\n' '{\"workspace\":\"/workspaces/project with spaces\"}'\n",
    );
    write_executable(
        &temp.path().join("bin/code"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/code-args\"\n",
    );
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "world")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-args")).unwrap(),
        "--\nars.world-host\n/usr/local/bin/wt-app-info\n"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("code-args")).unwrap(),
        "--remote\nssh-remote+ars.world-vs\n/workspaces/project with spaces\n"
    );
}

#[test]
fn remote_context_runs_the_same_protocol_over_openssh() {
    let (temp, path) = test_home(
        "[[contexts]]\nname = \"lab\"\nkind = \"bare_metal_ssh\"\nhost = \"wt-server\"\n",
        "#!/bin/sh\nexit 2\n",
    );
    write_executable(
        &temp.path().join("bin/ssh"),
        r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$HOME/ssh-args"
IFS= read -r start
printf '%s\n' '{"message":"ready","schema":1}'
printf '%s\n' '{"message":"exit","code":0}'
"#,
    );
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "ls")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-args")).unwrap(),
        "--\nwt-server\nwt-server\napi\n"
    );
}
