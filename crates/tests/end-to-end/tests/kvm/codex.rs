use super::*;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) struct CodexSessionFixture {
    pub(crate) marker: String,
    root: PathBuf,
}

impl CodexSessionFixture {
    pub(crate) fn new(name: &WorldName, config: &wt_server::ServerConfig) -> Self {
        let sessions = Path::new(config.codex_paths().sessions);
        let fixture_name = format!(".wt-kvm-e2e-{name}");
        let root = sessions.join(&fixture_name);
        let transcript_dir = root.join("2026/08/21");
        fs::create_dir_all(&transcript_dir).unwrap();
        let marker = format!("{fixture_name}/2026/08/21/rollout.jsonl");
        Self { marker, root }
    }
}

impl Drop for CodexSessionFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn assert_server_codex_auth_export(config: &wt_server::ServerConfig) -> String {
    let paths = config.codex_paths();
    let source = Path::new(paths.auth);
    let export_dir = Path::new(paths.auth_share);
    let export = export_dir.join("auth.json");
    let source_metadata = fs::symlink_metadata(source).unwrap();
    let export_metadata = fs::symlink_metadata(&export).unwrap();
    let directory_metadata = fs::symlink_metadata(export_dir).unwrap();
    assert!(source_metadata.file_type().is_file());
    assert!(export_metadata.file_type().is_file());
    assert!(directory_metadata.file_type().is_dir());
    assert_ne!(
        (source_metadata.dev(), source_metadata.ino()),
        (export_metadata.dev(), export_metadata.ino())
    );
    for metadata in [&source_metadata, &export_metadata, &directory_metadata] {
        assert_eq!(metadata.uid(), wt_guest::GUEST_UID);
        assert_eq!(metadata.gid(), wt_guest::GUEST_GID);
    }
    assert_eq!(source_metadata.mode() & 0o777, 0o600);
    assert_eq!(export_metadata.mode() & 0o777, 0o600);
    assert_eq!(directory_metadata.mode() & 0o777, 0o700);
    assert_eq!(sha256_file(source), sha256_file(&export));
    sha256_file(source)
}

pub(crate) fn verify_codex_auth_rotation(
    harness: &KvmHarness,
    name: &WorldName,
    expected_sha256: &str,
) {
    let paths = harness.config.codex_paths();
    let auth = Path::new(paths.auth);
    let export = Path::new(paths.auth_share).join("auth.json");
    let guest_inode = guest_output(
        harness,
        name,
        "stat -Lc %i /home/wt/.codex/auth.json",
        "read guest Codex auth inode",
    );
    let old_server_inode = fs::metadata(auth).unwrap().ino();
    let auth_bytes = fs::read(auth).unwrap();
    let mut replacement = tempfile::NamedTempFile::new_in(auth.parent().unwrap()).unwrap();
    replacement
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    replacement.write_all(&auth_bytes).unwrap();
    replacement.as_file().sync_all().unwrap();
    replacement.persist(auth).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while fs::metadata(auth).unwrap().ino() == old_server_inode
        || sha256_file(auth) != sha256_file(&export)
        || fs::metadata(auth).unwrap().gid() != wt_guest::GUEST_GID
        || fs::metadata(auth).unwrap().mode() & 0o777 != 0o600
    {
        assert!(
            Instant::now() < deadline,
            "Codex auth export did not refresh"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        assert_server_codex_auth_export(&harness.config),
        expected_sha256
    );
    run_guest(
        harness,
        name,
        &format!(
            "set -eu; for attempt in $(seq 1 100); do inode=$(stat -Lc %i /home/wt/.codex/auth.json); digest=$(sha256sum /home/wt/.codex/auth.json | awk '{{print $1}}'); if test \"$inode\" != '{}' && test \"$digest\" = '{}'; then test ! -w /home/wt/.codex/auth.json; exit 0; fi; sleep 0.1; done; exit 1",
            guest_inode.trim(), expected_sha256
        ),
        "observe rotated Codex auth in guest",
    );
}

fn sha256_file(path: &Path) -> String {
    let output = cmd!("sha256sum", "--", path).output().unwrap();
    ensure_success("hash Codex authentication", &output).unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

fn guest_output(harness: &KvmHarness, name: &WorldName, command: &str, action: &str) -> String {
    let output = guest_command(harness, name, command).output().unwrap();
    ensure_success(action, &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}
