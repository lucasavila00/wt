use std::fs;
use std::os::unix::fs::PermissionsExt;
use wt_client::cmd;

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
        &bin.join("wts"),
        &format!(
            r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"operation":"server_info"'*)
    printf '%s\n' '{{"protocol_version":13,"outcome":"ok","response":{{"response":"server_info","test_server":false,"build":{{"version":"test","commit":"0000000000000000000000000000000000000000"}}}}}}'
    ;;
  *'"operation":"list_worlds"'*)
    printf '%s\n' '{{"protocol_version":13,"outcome":"ok","response":{{"response":"worlds","worlds":[{{"world_id":"00000000-0000-0000-0000-000000000001","name":"world","owner":"tester","status":"{status}","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}]}}}}'
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
fn code_syncs_then_opens_the_qualified_direct_alias() {
    let (temp, path) = test_home("running");
    write_executable(
        &temp.path().join("bin/code"),
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$HOME/code-args\"\n",
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "world")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(temp.path().join("code-args")).unwrap(),
        "--remote\nssh-remote+ars.world-direct\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(managed.contains("Host ars.world-direct"));
}

#[test]
fn code_rejects_a_world_without_an_alias_before_launching_vscode() {
    let (temp, path) = test_home("stopped");
    write_executable(
        &temp.path().join("bin/code"),
        "#!/bin/sh\ntouch \"$HOME/code-started\"\n",
    );

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "ars.world")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @r###"
    wt: world ars.world has no managed SSH alias in status stopped
    "###);
    assert!(!temp.path().join("code-started").exists());
}

#[test]
fn code_propagates_the_vscode_cli_failure() {
    let (temp, path) = test_home("running");
    write_executable(&temp.path().join("bin/code"), "#!/bin/sh\nexit 23\n");

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "code", "ars.world")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @r###"
    wt: VS Code exited with exit status: 23
    "###);
}
