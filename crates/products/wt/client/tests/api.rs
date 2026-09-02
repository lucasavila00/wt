use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::process::{Output, Stdio};
use wt_client::cmd;

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn test_home() -> (tempfile::TempDir, std::ffi::OsString) {
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
        r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"name":"capacity"'*)
    printf '%s\n' '{"protocol_version":16,"outcome":"error","error":{"code":"capacity","message":"world CPU capacity is full","capacity":{"resource":"cpu","total":4,"reserved":4,"requested":2}}}'
    ;;
  *'"name":"duplicate"'*)
    printf '%s\n' '{"protocol_version":16,"outcome":"error","error":{"code":"conflict","message":"world already exists"}}'
    ;;
  *'"operation":"create_world"'*)
    printf '%s\n' '{"protocol_version":16,"event":"progress","message":"creating disk"}'
    printf '%s\n' '{"protocol_version":16,"outcome":"ok","response":{"response":"world","world":{"world_id":"00000000-0000-0000-0000-000000000001","name":"agent-1","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
    ;;
  *'"operation":"delete_world"'*)
    case "$request" in
      *'"world_id":"00000000-0000-0000-0000-000000000002"'*)
        printf '%s\n' '{"protocol_version":16,"outcome":"error","error":{"code":"not_found","message":"world not found"}}'
        ;;
      *)
        printf '%s\n' '{"protocol_version":16,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000001"}}'
        ;;
    esac
    ;;
  *) exit 2 ;;
esac
"#,
    );
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    (temp, path)
}

fn call_api(temp: &tempfile::TempDir, path: &std::ffi::OsString, input: &str) -> Output {
    let mut child = cmd!(env!("CARGO_BIN_EXE_wt"), "api")
        .env("HOME", temp.path())
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn creates_a_world_with_the_versioned_json_contract() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"create_world","name":"agent-1","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"ok","response":{"response":"world","world":{"world_id":"00000000-0000-0000-0000-000000000001","name":"agent-1","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}
    "###);
}

#[test]
fn deletes_a_world_with_the_versioned_json_contract() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000001"}"#,
    );

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000001"}}
    "###);
}

#[test]
fn rejects_unknown_request_fields() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000001","extra":true}"#,
    );

    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"error","error":{"code":"invalid_request","message":"invalid JSON request"}}
    "###);
}

#[test]
fn reports_server_rejections_as_nonzero_json_errors() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"create_world","name":"duplicate","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert_eq!(output.status.code(), Some(1));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"error","error":{"code":"conflict","message":"world already exists"}}
    "###);
}

#[test]
fn deletes_an_already_absent_world_successfully() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000002"}"#,
    );

    assert!(output.status.success());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000002"}}
    "###);
}

#[test]
fn returns_structured_capacity_details() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"context":"ars","operation":"create_world","name":"capacity","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert_eq!(output.status.code(), Some(1));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"api_version":1,"outcome":"error","error":{"code":"capacity","message":"world CPU capacity is full","details":{"kind":"capacity","resource":"cpu","total":4,"reserved":4,"requested":2}}}
    "###);
}
