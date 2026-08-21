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
            home.path().join(".ssh/id_ed25519.pub"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH4Ma5yVqds1tDCNyJzHbbXZdD/RvXWz10hkWHFWhNpw\n",
        )
        .unwrap();
        let server = r#"#!/bin/sh
request=$(cat)
case "$request" in
  *'"operation":"list_codex_sessions"'*)
    if test -f "$HOME/codex-active"; then
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"codex_sessions","sessions":[{"session_id":"123e4567-e89b-12d3-a456-426614174000","rollout_updated_at_unix_ms":10,"observations":[{"world_id":"00000000-0000-0000-0000-000000000001","world_name":"existing","cwd":"/home/wt/project","state":"working","target":{"tmux_session":"wt-host","pane_id":"%1"},"received_at_unix_ms":20}]}]}}'
    else
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"codex_sessions","sessions":[]}}'
    fi
    ;;
  *'"operation":"list"'*)
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"existing","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}]}}'
    ;;
  *) exit 2 ;;
esac
"#
        .replace(
            "@PROTOCOL_VERSION@",
            &wt_control_protocol::PROTOCOL_VERSION.to_string(),
        );
        write_executable(&bin.join("wt-server"), &server);
        write_executable(
            &bin.join("ssh"),
            "#!/bin/sh\nstty -echo\nprintf 'session: %s\n' \"$2\"\nexec cat\n",
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
                ("PATH", self.path.clone()),
            ],
        )
    }
}

#[test]
fn command_palette_opens_the_world_form_and_escape_cancels() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No Codex sessions")?
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?
        .press(Key::Escape)?
        .wait_for_text("No Codex sessions")?
        .wait_for_text_gone("Create world")?;
    Ok(())
}

#[test]
fn codex_sessions_refresh_after_startup() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen.wait_for_text("No Codex sessions")?;
    fs::write(fixture.home.path().join("codex-active"), "").unwrap();
    screen
        .wait_for_text("/home/wt/project")?
        .wait_for_text("wt-host:%1")?
        .wait_for_quiet(Duration::from_millis(50))?;
    Ok(())
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
