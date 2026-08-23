mod support;

use anyhow::Result;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use support::{Key, Screen};

static SHELL_TEST: Mutex<()> = Mutex::new(());

struct Fixture {
    _guard: MutexGuard<'static, ()>,
    home: tempfile::TempDir,
    path: OsString,
}

impl Fixture {
    fn new() -> Self {
        let guard = SHELL_TEST.lock().unwrap();
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
  *'"operation":"server_info"'*)
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"server_info","test_server":false,"build":{"version":"test","commit":"0000000000000000000000000000000000000000"}}}'
    ;;
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
  *'"operation":"create"'*)
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"event":"progress","message":"Waiting for the guest transport..."}'
    sleep 5
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"error","error":{"code":"backend","message":"fixture stopped creation"}}'
    ;;
  *'"operation":"delete"'*)
    sleep 5
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"deleted","name":"existing"}}'
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
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/ssh-args\"\ncontrol=\nprevious=\nfor argument do\n  if test \"$previous\" = -S; then control=$argument; fi\n  previous=$argument\n  target=$argument\ndone\ncase \"$*\" in\n  *'-O check'*) test -e \"$control\" ;;\n  *'focus-pane'*'%1') printf 'wt-host:%%1:123e4567-e89b-12d3-a456-426614174000:0\\n' ;;\n  *'-M -S'*) touch \"$control\"; stty -echo; printf 'session: %s\\n' \"$target\"; exec cat ;;\n  *) exit 2 ;;\nesac\n",
        );
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap();
        Self {
            _guard: guard,
            home,
            path,
        }
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
            Duration::from_secs(20),
        )
    }
}

#[test]
fn command_palette_opens_the_world_form_and_ok_is_clickable() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex sessions")?
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?
        .click(13, 11)?
        .wait_for_text("Review")?
        .press(Key::Escape)?
        .wait_for_text("No live Codex sessions")?
        .wait_for_text_gone("Create world")?;
    Ok(())
}

#[test]
fn world_creation_runs_behind_a_live_progress_notification() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex sessions")?
        .press(Key::Function(1))?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?
        .press(Key::Tab)?
        .press(Key::Enter)?
        .wait_for_quiet(Duration::from_millis(50))?
        .type_text("background")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?
        .wait_for_text("Waiting for the guest transport...")?
        .wait_for_text("local.background")?
        .click(97, 1)?
        .wait_for_text_gone("Waiting for the guest transport...")?
        .wait_for_text("PROVISIONING")?
        .wait_for_text("Creation in progress")?;
    Ok(())
}

#[test]
fn world_creation_progress_can_be_hidden_without_blocking_navigation() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex sessions")?
        .press(Key::Function(1))?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?
        .press(Key::Tab)?
        .press(Key::Enter)?
        .wait_for_quiet(Duration::from_millis(50))?
        .type_text("background")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?
        .wait_for_text("Waiting for the guest transport...")?
        .click(97, 1)?
        .wait_for_text_gone("Waiting for the guest transport...")?
        .click(2, 1)?
        .wait_for_text("No Codex sessions")?
        .wait_for_text("Creation did not complete")?;
    Ok(())
}

#[test]
fn world_deletion_progress_can_be_hidden_without_blocking_navigation() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex sessions")?
        .press(Key::Function(1))?
        .type_text("delete")?
        .press(Key::Enter)?
        .wait_for_text("Delete world")?
        .press(Key::Enter)?
        .wait_for_text("Delete world?")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text("WT is deleting the world")?
        .wait_for_text("local.existing")?
        .press(Key::Tab)?
        .press(Key::Tab)?
        .wait_for_text("Worlds")?
        .click(97, 1)?
        .wait_for_text_gone("WT is deleting the world")?;
    Ok(())
}

#[test]
fn codex_sessions_refresh_after_startup() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex sessions")?
        .press(Key::Tab)?
        .wait_for_text("No Codex sessions")?;
    fs::write(fixture.home.path().join("codex-active"), "").unwrap();
    screen
        .wait_for_text("/home/wt/project")?
        .wait_for_text("wt-host:%1")?
        .wait_for_quiet(Duration::from_millis(50))?;
    Ok(())
}

#[test]
fn opening_a_codex_session_reuses_the_world_ssh_connection() -> Result<()> {
    let fixture = Fixture::new();
    fs::write(fixture.home.path().join("codex-active"), "").unwrap();
    let mut screen = fixture.screen()?;
    screen
        .press(Key::Tab)?
        .wait_for_text("/home/wt/project")?
        .press(Key::Enter)?
        .wait_for_text("session: local.existing")?;

    let calls = fs::read_to_string(fixture.home.path().join("ssh-args"))?;
    let mut lines = calls.lines();
    let playback = lines.next().unwrap();
    let focus = lines.find(|line| line.contains("focus-pane")).unwrap();
    assert!(playback.starts_with("-M -S "));
    assert!(focus.contains("ProxyCommand=/bin/false"));
    assert_eq!(control_path(playback), control_path(focus));
    Ok(())
}

#[test]
fn live_activity_reuses_the_open_world_stream() -> Result<()> {
    let fixture = Fixture::new();
    fs::write(fixture.home.path().join("codex-active"), "").unwrap();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("Live sessions · Experimental")?
        .wait_for_text("session: local.existing")?
        .wait_for_quiet(Duration::from_millis(50))?;
    let calls = fs::read_to_string(fixture.home.path().join("ssh-args"))?;
    let mut lines = calls.lines();
    let playback = lines.next().unwrap();
    let focus = lines.find(|line| line.contains("focus-pane")).unwrap();
    assert_eq!(control_path(playback), control_path(focus));
    assert!(calls.contains("wt-codex-integration focus-pane"));
    assert!(calls.contains("123e4567-e89b-12d3-a456-426614174000 wt-host %1"));
    Ok(())
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn control_path(command: &str) -> &str {
    let mut arguments = command.split_whitespace();
    arguments
        .find(|argument| *argument == "-S")
        .and_then(|_| arguments.next())
        .expect("SSH command has a control socket")
}
