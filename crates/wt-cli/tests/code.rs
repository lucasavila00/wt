use std::fs;
use std::os::unix::fs::PermissionsExt;
use wt_command::cmd;

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn test_home(status: &str) -> (tempfile::TempDir, std::ffi::OsString) {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    write_executable(
        &bin.join("wt-server"),
        &format!(
            r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"operation":"list"'*)
    printf '%s\n' '{{"protocol_version":1,"outcome":"ok","response":{{"response":"instances","instances":[{{"id":"00000000-0000-0000-0000-000000000001","name":"jsdev","owner":"tester","status":"{status}","source":"git@example.test:group/repo.git","guest_ip":"192.0.2.2","ssh":{{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}},"app_ssh":{{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}}}]}}}}'
    ;;
  *) exit 2 ;;
esac
"#
        ),
    );
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    (temp, path)
}

#[test]
fn code_opens_the_live_container_workspace_through_the_qualified_alias() {
    let (temp, path) = test_home("running");
    let bin = temp.path().join("bin");
    write_executable(
        &bin.join("ssh"),
        r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$HOME/ssh-args"
printf '%s\n' '{"container":"abc","workspace":"/workspaces/project with spaces","user":"vscode","address":"172.18.0.3"}'
"#,
    );
    write_executable(
        &bin.join("code"),
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$HOME/code-args\"\n",
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-args")).unwrap(),
        "--\nars.jsdev-host\n/usr/local/bin/wt-app-info\n"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("code-args")).unwrap(),
        "--remote\nssh-remote+ars.jsdev-vs\n/workspaces/project with spaces\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(managed.contains("Host ars.jsdev-vs"));
}

#[test]
fn code_rejects_a_world_that_is_not_running_before_launching_processes() {
    let (temp, path) = test_home("provisioning");
    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "ars.jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @r###"
    wt: world ars.jsdev is provisioning; VS Code can only open a running world
    "###);
    assert!(!temp.path().join("ssh-args").exists());
    assert!(!temp.path().join("code-args").exists());
}

#[test]
fn code_reports_invalid_app_information_without_starting_vscode() {
    let (temp, path) = test_home("running");
    let bin = temp.path().join("bin");
    write_executable(&bin.join("ssh"), "#!/bin/sh\nprintf '%s\\n' 'not-json'\n");
    write_executable(
        &bin.join("code"),
        "#!/bin/sh\ntouch \"$HOME/code-started\"\n",
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "ars.jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @r###"
    wt: decode app information for ars.jsdev: expected ident at line 1 column 2
    "###);
    assert!(!temp.path().join("code-started").exists());
}

#[test]
fn code_propagates_the_vscode_cli_failure() {
    let (temp, path) = test_home("running");
    let bin = temp.path().join("bin");
    write_executable(
        &bin.join("ssh"),
        "#!/bin/sh\nprintf '%s\\n' '{\"workspace\":\"/workspaces/project\"}'\n",
    );
    write_executable(&bin.join("code"), "#!/bin/sh\nexit 23\n");

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "ars.jsdev")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @r###"
    wt: VS Code exited with exit status: 23
    "###);
}
