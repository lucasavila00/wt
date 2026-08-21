mod support;

use anyhow::Result;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use support::{Key, Screen};

struct Fixture {
    home: tempfile::TempDir,
    path: OsString,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        fs::create_dir_all(home.path().join(".wt")).unwrap();
        fs::create_dir_all(home.path().join(".ssh")).unwrap();
        fs::create_dir_all(home.path().join(".config/wt")).unwrap();
        fs::write(
            home.path().join(".wt/config.toml"),
            "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".gitconfig"),
            "[user]\n\tname = Test User\n\temail = test@example.test\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".config/wt/cloud-init.yaml"),
            "#cloud-config\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".ssh/id_ed25519.pub"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH4Ma5yVqds1tDCNyJzHbbXZdD/RvXWz10hkWHFWhNpw\n",
        )
        .unwrap();
        let server = r#"#!/bin/sh
request=$(cat)
case "$request" in
  *'"operation":"list_codex_sessions"'*)
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"codex_sessions","sessions":[]}}'
    ;;
  *'"operation":"create"'*)
    : > "$HOME/created"
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000002","name":"new-world","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"new-world/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.3","ssh":{"user":"wt","host":"192.0.2.3","port":22,"host_keys":["ssh-ed25519 AAAANEW guest"]}}}}'
    ;;
  *'"operation":"list"'*)
    if test -f "$HOME/created"; then
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"existing","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"existing/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}},{"id":"00000000-0000-0000-0000-000000000002","name":"new-world","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"new-world/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.3","ssh":{"user":"wt","host":"192.0.2.3","port":22,"host_keys":["ssh-ed25519 AAAANEW guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAANEWAPP app"]}}]}}'
    else
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"existing","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"existing/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}]}}'
    fi
    ;;
  *) exit 2 ;;
esac
"#;
        let server = server.replace(
            "@PROTOCOL_VERSION@",
            &wt_control_protocol::PROTOCOL_VERSION.to_string(),
        );
        write_executable(&bin.join("wt-server"), &server);
        write_executable(
            &bin.join("ssh"),
            "#!/bin/sh\nstty -echo\nprintf 'session: %s\\n' \"$2\"\ncat\n",
        );
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap();
        Self { home, path }
    }

    fn screen(&self) -> Result<Screen> {
        Screen::launch(
            env!("CARGO_BIN_EXE_wt"),
            &["shell"],
            self.home.path(),
            &[
                ("HOME", self.home.path().as_os_str().to_os_string()),
                (
                    "XDG_CONFIG_HOME",
                    self.home.path().join(".config").into_os_string(),
                ),
                ("PATH", self.path.clone()),
            ],
        )
    }
}

#[test]
fn command_palette_opens_the_shared_dev_form_and_escape_cancels() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;

    screen
        .wait_for_text("No Codex sessions")?
        .press(Key::Tab)?
        .wait_for_text("World management")?
        .press(Key::Tab)?
        .wait_for_text("No Codex sessions")?
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .type_text("dev")?
        .press(Key::Enter)?
        .wait_for_text("Create development world")?
        .press(Key::Function(5))?
        .wait_for_text("session: local.existing")?
        .wait_for_text_gone("Create development world")?
        .press(Key::Function(5))?
        .wait_for_text("F5: disable navbar")?
        .press(Key::Up)?
        .wait_for_text("Create development world")?
        .wait_for_quiet(Duration::from_millis(100))?;
    screen
        .press(Key::Escape)?
        .wait_for_text("No Codex sessions")?
        .wait_for_text_gone("Create development world")?;
    Ok(())
}

#[test]
fn one_shortcut_can_open_the_shared_host_form() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;

    screen
        .wait_for_text("No Codex sessions")?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .press(Key::Enter)?
        .wait_for_text("Create host world")?
        .wait_for_text("cloud-init.yaml")?
        .press(Key::Escape)?
        .wait_for_text("No Codex sessions")?;
    Ok(())
}

#[test]
fn shift_f5_disables_and_restores_the_f5_override() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;

    screen
        .wait_for_text("No Codex sessions")?
        .press(Key::Function(5))?
        .wait_for_text("F5: enable navbar")?
        .press(Key::ShiftFunction(5))?
        .wait_for_text("F5 disabled")?
        .wait_for_text("Shift+F5: enable")?
        .press(Key::Function(5))?
        .wait_for_text("F5 disabled")?
        .press(Key::ShiftFunction(5))?
        .wait_for_text("F5: enable navbar")?
        .wait_for_text_gone("F5 disabled")?;
    Ok(())
}

#[test]
fn submitted_form_adds_and_activates_a_persistent_world_session() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;

    screen
        .wait_for_text("No Codex sessions")?
        .press(Key::Function(1))?
        .type_text("dev")?
        .press(Key::Enter)?
        .wait_for_text("Create development world")?
        .press(Key::Tab)?
        .type_text("new-world")?
        .press(Key::Enter)?
        .type_text("git@example.test:repo.git")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?
        .wait_for_text("Creating world")?
        .wait_for_text("session: local.new-world")?
        .press(Key::Function(5))?
        .wait_for_text("local.new-world (2/2)")?
        .wait_for_text("F5: disable navbar")?;
    assert!(fixture.home.path().join("created").exists());
    Ok(())
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
