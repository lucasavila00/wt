use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn install_and_update_keep_interactive_codex_and_user_state() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home with spaces");
    let fixtures = temp.path().join("fixtures");
    let bin = temp.path().join("bin");
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    fs::create_dir_all(&fixtures).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let preserved = [
        ".local/bin/codex",
        ".codex/auth.json",
        ".codex/config.toml",
        ".codex/sessions/session.jsonl",
        ".profile",
        ".bashrc",
    ];
    for name in preserved {
        fs::write(home.join(name), format!("original {name}\n")).unwrap();
    }
    executable(&fixtures.join("agapi"), "#!/bin/sh\necho agapi\n");
    // Model the upstream installer's configurable binary/package destinations.
    executable(
        &fixtures.join("installer"),
        r#"#!/bin/sh
set -eu
test "$CODEX_NON_INTERACTIVE" = 1
test "$CODEX_HOME" = "$USER_HOME/.local/share/agapi/codex"
test "$HOME" != "$USER_HOME"
test "$CODEX_INSTALL_DIR" = "$CODEX_HOME/bin"
test "${PATH%%:*}" = "$CODEX_INSTALL_DIR"
printf 'installer profile change\n' > "$HOME/.bashrc"
mkdir -p "$CODEX_HOME/packages/standalone/current/bin" "$CODEX_INSTALL_DIR"
printf '#!/bin/sh\necho codex-cli %s\n' "$CODEX_RELEASE" > "$CODEX_HOME/packages/standalone/current/bin/codex"
chmod +x "$CODEX_HOME/packages/standalone/current/bin/codex"
ln -sfn "$CODEX_HOME/packages/standalone/current/bin/codex" "$CODEX_INSTALL_DIR/codex"
"#,
    );
    executable(
        &bin.join("curl"),
        r#"#!/bin/sh
set -eu
case "$2" in
    */agapi-x86_64-linux.tar.gz) source=agapi-x86_64-linux.tar.gz ;;
    */SHA256SUMS) source=SHA256SUMS ;;
    https://chatgpt.com/codex/install.sh) source=installer ;;
    *) exit 1 ;;
esac
test "$3" = -o
cp "$FIXTURES/$source" "$4"
"#,
    );
    for version in ["0.153.3", "0.153.4"] {
        fs::write(fixtures.join("codex-version"), version).unwrap();
        assert!(Command::new("tar")
            .current_dir(&fixtures)
            .args([
                "-czf",
                "agapi-x86_64-linux.tar.gz",
                "agapi",
                "codex-version"
            ])
            .status()
            .unwrap()
            .success());
        let checksum = Command::new("sha256sum")
            .current_dir(&fixtures)
            .arg("agapi-x86_64-linux.tar.gz")
            .output()
            .unwrap();
        assert!(checksum.status.success());
        fs::write(fixtures.join("SHA256SUMS"), checksum.stdout).unwrap();
        let output = Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../scripts/install-agapi"
            ))
            .arg("0.1.0")
            .env("HOME", &home)
            .env("USER_HOME", &home)
            .env("FIXTURES", &fixtures)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for name in preserved {
            assert_eq!(
                fs::read_to_string(home.join(name)).unwrap(),
                format!("original {name}\n")
            );
        }
        let installed = Command::new(home.join(".local/share/agapi/codex/bin/codex"))
            .arg("--version")
            .output()
            .unwrap();
        assert!(installed.status.success());
        assert_eq!(
            String::from_utf8(installed.stdout).unwrap().trim(),
            format!("codex-cli {version}")
        );
        assert!(home.join(".local/bin/agapi").is_file());
    }
}
