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
  *'"request_id":"11111111-1111-4111-8111-111111111111"'*'"operation":"list_world_mail"'*) ;;
  *'"request_id":"11111111-1111-4111-8111-111111111111"'*'"request_hash":"'*) ;;
  *) exit 3 ;;
esac
case "$request" in
  *'"expected_server_id":"22222222-2222-4222-8222-222222222222"'*'"name":"agent-1"'*) ;;
  *'"name":"agent-1"'*) exit 4 ;;
  *) ;;
esac
case "$request" in
  *'"name":"metadata"'*)
    printf '%s\n' '{"protocol_version":21,"request_id":"33333333-3333-4333-8333-333333333333","server_id":"22222222-2222-4222-8222-222222222222","outcome":"error","error":{"code":"conflict","message":"ignored"}}'
    ;;
  *'"name":"capacity"'*)
    printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"error","error":{"code":"capacity","message":"world CPU capacity is full","retryable":true,"capacity":{"resource":"cpu","total":4,"reserved":4,"requested":2}}}'
    ;;
  *'"name":"duplicate"'*)
    printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"error","error":{"code":"conflict","message":"world already exists"}}'
    ;;
  *'"operation":"create_world"'*)
    printf '%s\n' '{"protocol_version":21,"event":"progress","message":"creating disk"}'
    printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"ok","response":{"response":"world","world":{"world_id":"00000000-0000-0000-0000-000000000001","name":"agent-1","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
    ;;
  *'"operation":"list_world_mail"'*)
    printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"ok","response":{"response":"world_mail","messages":[{"id":7,"world_id":"00000000-0000-0000-0000-000000000001","thread_id":"thread-123","turn_id":"turn-456","pane_id":"%7","created_at_unix_ms":1800000000000,"kind":"completed","message":"done"}],"high_water_id":7}}'
    ;;
  *'"operation":"delete_world"'*)
    case "$request" in
      *'"world_id":"00000000-0000-0000-0000-000000000002"'*)
        printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000002"}}'
        ;;
      *)
        printf '%s\n' '{"protocol_version":21,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000001"}}'
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
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","expected_server_id":"22222222-2222-4222-8222-222222222222","context":"ars","operation":"create_world","name":"agent-1","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"ok","api_version":1,"expires_at_unix_ms":2592000100,"request_id":"11111111-1111-4111-8111-111111111111","result":{"world":{"disk_gib":32,"guest_ip":"192.0.2.2","memory_mib":4096,"name":"agent-1","ssh":{"host":"192.0.2.2","host_keys":["ssh-ed25519 AAAATEST guest"],"port":22,"user":"wt"},"status":"running","vcpus":2,"world_id":"00000000-0000-0000-0000-000000000001"}},"server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn deletes_a_world_with_the_versioned_json_contract() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000001"}"#,
    );

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"ok","api_version":1,"expires_at_unix_ms":2592000100,"request_id":"11111111-1111-4111-8111-111111111111","result":{"world_id":"00000000-0000-0000-0000-000000000001"},"server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn reads_attributed_world_mail() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"read_world_mail","world_id":"00000000-0000-0000-0000-000000000001","after_message_id":0,"limit":10}"#,
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"ok","api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","result":{"high_water_message_id":7,"messages":[{"created_at_unix_ms":1800000000000,"kind":"completed","message_id":7,"pane_id":"%7","text":"done","thread_id":"thread-123","turn_id":"turn-456","world_id":"00000000-0000-0000-0000-000000000001"}]},"server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn rejects_unknown_request_fields() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000001","extra":true}"#,
    );

    assert!(!output.status.success());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"error","api_version":1,"error":{"code":"invalid_request","message":"invalid JSON request","retryable":false}}
    "###);
}

#[test]
fn reports_server_rejections_as_nonzero_json_errors() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"create_world","name":"duplicate","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert_eq!(output.status.code(), Some(1));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"error","api_version":1,"error":{"code":"conflict","message":"world already exists","retryable":false},"expires_at_unix_ms":2592000100,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn deletes_an_already_absent_world_successfully() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"delete_world","world_id":"00000000-0000-0000-0000-000000000002"}"#,
    );

    assert!(output.status.success());
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"ok","api_version":1,"expires_at_unix_ms":2592000100,"request_id":"11111111-1111-4111-8111-111111111111","result":{"world_id":"00000000-0000-0000-0000-000000000002"},"server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn returns_structured_capacity_details() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"create_world","name":"capacity","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert_eq!(output.status.code(), Some(1));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"error","api_version":1,"error":{"code":"capacity","details":{"kind":"capacity","requested":2,"reserved":4,"resource":"cpu","total":4},"message":"world CPU capacity is full","retryable":true},"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}

#[test]
fn rejects_changed_server_response_metadata() {
    let (temp, path) = test_home();
    let output = call_api(
        &temp,
        &path,
        r#"{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","context":"ars","operation":"create_world","name":"metadata","vcpus":2,"memory_mib":4096,"disk_gib":32,"git_user_name":"Ada Lovelace","git_user_email":"ada@example.com"}"#,
    );

    assert_eq!(output.status.code(), Some(1));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap(), @r###"
    {"outcome":"error","api_version":1,"error":{"code":"internal_error","message":"server omitted or changed API request metadata","retryable":false},"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222"}
    "###);
}
