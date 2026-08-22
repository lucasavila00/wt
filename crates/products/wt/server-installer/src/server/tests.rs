use super::*;

struct CapturedRunner;

impl Runner for CapturedRunner {
    fn output(&self, mut command: std::process::Command) -> Result<std::process::Output> {
        Ok(command.output()?)
    }
}

struct FixedPassphrase(&'static str);

impl PassphrasePrompt for FixedPassphrase {
    fn read(
        &self,
        _kind: &str,
        _path: &Path,
        _private_key: &ssh_key::PrivateKey,
    ) -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(self.0.to_owned()))
    }
}

fn provider_config(temp: &Path, passphrase: &str) -> AgentToolsProviderInstallConfig {
    let private = temp.join("id_ed25519");
    let output = cmd!(
        "ssh-keygen",
        "-q",
        "-t",
        "ed25519",
        "-N",
        passphrase,
        "-f",
        &private,
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    let public = private.with_extension("pub");
    let token = temp.join("token");
    fs::write(&token, "test-token\n").unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    AgentToolsProviderInstallConfig {
        host: "github.com".to_owned(),
        api_token_file: token,
        ssh_private_key_file: private,
        ssh_public_key_file: public,
    }
}

#[test]
fn provider_credentials_are_validated_and_unlocked() {
    let temp = tempfile::tempdir().unwrap();
    let provider = provider_config(temp.path(), "");
    let prepared = prepare_provider_credentials(
        &CapturedRunner,
        &FixedPassphrase("unused"),
        "github",
        &provider,
    )
    .unwrap();
    let prepared = ssh_key::PrivateKey::read_openssh_file(prepared.private_key.path()).unwrap();
    assert!(!prepared.is_encrypted());
}

#[test]
fn encrypted_provider_key_is_unlocked_without_modifying_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let provider = provider_config(temp.path(), "correct horse battery staple");
    let original = fs::read(&provider.ssh_private_key_file).unwrap();
    let encrypted = ssh_key::PrivateKey::from_openssh(&original).unwrap();
    assert!(encrypted.is_encrypted());
    assert_eq!(
        validate_passphrase(&encrypted, "wrong passphrase"),
        Err("That passphrase did not unlock this SSH key. Try again.")
    );
    assert_eq!(
        validate_passphrase(&encrypted, "correct horse battery staple"),
        Ok(())
    );

    let prepared = prepare_provider_credentials(
        &CapturedRunner,
        &FixedPassphrase("correct horse battery staple"),
        "github",
        &provider,
    )
    .unwrap();

    assert_eq!(fs::read(&provider.ssh_private_key_file).unwrap(), original);
    let prepared = ssh_key::PrivateKey::read_openssh_file(prepared.private_key.path()).unwrap();
    assert!(!prepared.is_encrypted());
    assert_eq!(
        prepared.public_key().key_data(),
        encrypted.public_key().key_data()
    );
}

#[test]
fn setup_messages_explain_the_operation() {
    insta::assert_snapshot!(
        ssh_key_passphrase_context("github", Path::new("/home/wt/.ssh/id_ed25519")),
        @"
    GitHub SSH key is passphrase-protected

    Key: /home/wt/.ssh/id_ed25519
    WT needs an unlocked copy so the local agent tool gateway can fetch and push.
    The original key will not be changed. WT verifies the key pair, encrypts the
    unlocked copy as a systemd credential, and removes the temporary copy.
    "
    );
    insta::assert_snapshot!(phase_message("Preparing Git provider credentials"), @"==> Preparing Git provider credentials");
    insta::assert_snapshot!(success_message(Path::new("./server.toml")), @"
    WT server is ready.
    Config: ./server.toml
    Services started: wt-server, wt-agent-tool-gateway
    Next: configure a WT client, then run `wt new`.
    ");
}

#[test]
fn credential_paths_do_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::write(&target, "secret").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let link = temp.path().join("link");
    symlink(&target, &link).unwrap();
    assert!(read_owned_file(&link, true, "credential").is_err());
}

#[test]
fn config_drift_message_explains_recovery() {
    insta::assert_snapshot!(
        config_drift_message(Path::new("./server.toml")),
        @"
    installed server config differs from install input

    Installed runtime config: /etc/wt/server.toml
    Install input: ./server.toml

    /etc/wt/server.toml is the runtime contract, materialized from install input.
    The installer leaves a differing file in place.

    Accidental change: re-run with the install input that produced the current server:
      scripts/install-server --config ./server.toml

    Intentional change: clear WT server state, then reinstall:
      make clear   # or: scripts/clear
      scripts/install-server --config ./server.toml

    `make clear` destroys every wt-* domain and removes generated runtime state
    (config, worlds, grants, database, and generated SSH inventory). It keeps
    the verified golden image, installed services and credentials, source downloads,
    and caches.
    "
    );
}

#[test]
fn service_unit_drift_requires_a_runtime_reset() {
    let path = Path::new("/etc/systemd/system/wt-server.service");
    assert!(!service_unit_needs_replacement(path, b"same", b"same", false).unwrap());
    assert!(service_unit_needs_replacement(path, b"old", b"new", true).unwrap());
    insta::assert_snapshot!(
        service_unit_needs_replacement(path, b"old", b"new", false)
            .unwrap_err()
            .to_string(),
        @"service unit drift at /etc/systemd/system/wt-server.service; run make clear before reinstalling"
    );
}

#[test]
fn services_use_the_canonical_identity() {
    let input = toml::from_str::<InstallInput>(
        r#"
version = 1
test_server = false
[capacity]
version = 1
limits = { vcpus = 4, memory_mib = 8192, disk_gib = 128 }
[image]
source_url = "https://example.test/image"
source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
path = "/var/lib/wt/retained.qcow2"
build_memory_mib = 1024
build_vcpus = 1
build_disk_gib = 8
[libvirt]
network = "default"
worlds_dir = "/var/lib/wt/worlds"
[agent_tools.github]
host = "github.com"
api_token_file = "/tmp/github.token"
ssh_private_key_file = "/tmp/id_ed25519"
ssh_public_key_file = "/tmp/id_ed25519.pub"
[guest]
boot_timeout_seconds = 30
readiness_timeout_seconds = 30
[install]
binary_dir = "/opt/wt bin"
"#,
    )
    .unwrap();
    let server = input.materialize();
    let unit = String::from_utf8(server_service(&server)).unwrap();
    insta::assert_snapshot!(unit, @r###"
    [Unit]
    Description=WT control-plane daemon
    Requires=wt-codex-integration-auth.service
    Wants=network-online.target wt-agent-tool-gateway.service wt-codex-integration-auth.path
    After=network-online.target libvirtd.service wt-agent-tool-gateway.service wt-codex-integration-auth.service

    [Service]
    Type=simple
    User=wt
    Group=wt
    Environment="HOME=/home/wt"
    Environment="WT_AGENT_TOOL_VSOCK_PORT=18017"
    ExecStart="/opt/wt bin/wt-server" serve
    Restart=on-failure
    RuntimeDirectory=wt
    RuntimeDirectoryMode=0700
    UMask=0077

    [Install]
    WantedBy=multi-user.target
    "###);
    let codex_auth = String::from_utf8(codex_auth_service()).unwrap();
    insta::assert_snapshot!(codex_auth, @r###"
    [Unit]
    Description=Refresh the WT Codex authentication share

    [Service]
    Type=oneshot
    User=wt
    Group=wt
    Environment="HOME=/home/wt"
    ExecStart=/usr/local/libexec/wt-codex-integration-auth-share
    UMask=0077
    "###);
    insta::assert_snapshot!(String::from_utf8(codex_auth_path_unit()).unwrap(), @r###"
    [Unit]
    Description=Watch the WT Codex authentication file

    [Path]
    PathChanged=/home/wt/.codex/auth.json
    Unit=wt-codex-integration-auth.service

    [Install]
    WantedBy=multi-user.target
    "###);
    let gateway = String::from_utf8(gateway_service(&input, &server)).unwrap();
    insta::assert_snapshot!("gateway_service", gateway);
}
