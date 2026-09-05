use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn image_installs_interactive_codex_without_replacing_user_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let home = root.join("home");
    let bin = root.join("bin");
    fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    for name in ["auth.json", "config.toml", "sessions/thread.jsonl"] {
        fs::write(home.join(".codex").join(name), name).unwrap();
    }
    fs::write(
        root.join("wt-image-build.env"),
        format!(
            "WT_USER=wt\nWT_HOME='{}'\nCODEX_RELEASE=1.2.3\n",
            home.display()
        ),
    )
    .unwrap();
    // Only replace privileged paths and external commands; execute the image asset.
    let script = include_str!("../../../../../assets/world/shared/install-codex.sh")
        .replace("/var/tmp", root.to_str().unwrap())
        .replace("/usr/local/bin", bin.to_str().unwrap());
    fs::write(root.join("install.sh"), script).unwrap();
    executable(&bin.join("runuser"), "#!/bin/sh\nshift 3\nexec \"$@\"\n");
    executable(&bin.join("curl"), "#!/bin/sh\ncp \"$FIXTURE\" \"$4\"\n");
    let fixture = root.join("upstream-installer");
    executable(
        &fixture,
        r#"#!/bin/sh
set -eu
bin_dir=${CODEX_INSTALL_DIR:-$HOME/.local/bin}
mkdir -p "$bin_dir" "$CODEX_HOME/packages"
printf '#!/bin/sh\necho codex-cli %s\n' "$CODEX_RELEASE" > "$CODEX_HOME/packages/codex"
chmod +x "$CODEX_HOME/packages/codex"
ln -sfn "$CODEX_HOME/packages/codex" "$bin_dir/codex"
printf '%s\n' "$CODEX_RELEASE" > "$HOME/.profile"
"#,
    );
    let output = Command::new("sh")
        .arg(root.join("install.sh"))
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("FIXTURE", &fixture)
        .env("TMPDIR", root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(bin.join("codex"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "codex-cli 1.2.3\n"
    );
    assert_eq!(
        fs::read_to_string(home.join(".profile")).unwrap(),
        "1.2.3\n"
    );
    for name in ["auth.json", "config.toml", "sessions/thread.jsonl"] {
        assert_eq!(
            fs::read_to_string(home.join(".codex").join(name)).unwrap(),
            name
        );
    }
}
