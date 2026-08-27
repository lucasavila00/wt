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
  *'"operation":"list_pane_observations"'*)
    if test -f "$HOME/pane-observed"; then
      observed_at=$(date +%s%3N)
      changed_at=$((observed_at - 16000))
      printf '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"pane_observations","panes":[{"world_id":"00000000-0000-0000-0000-000000000001","world_name":"existing","created_at_unix_ms":10,"tmux_session":"wt-host","pane_id":"%%1","cwd":"/home/wt","changed_at_unix_ms":%s,"observed_at_unix_ms":%s}]}}\n' "$changed_at" "$observed_at"
    else
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"pane_observations","panes":[]}}'
    fi
    ;;
  *'"operation":"list_git_activity"'*)
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"git_activity","activity":[{"id":2,"world_id":"00000000-0000-0000-0000-000000000001","world_name":"existing","recorded_at_unix_ms":20,"kind":"service","provider_host":"github.com","repository":"owner/write","git_service":"git-receive-pack"},{"id":1,"world_id":"00000000-0000-0000-0000-000000000001","world_name":"existing","recorded_at_unix_ms":10,"kind":"service","provider_host":"github.com","repository":"owner/read","git_service":"git-upload-pack"}]}}'
    ;;
  *'"operation":"list_worlds"'*)
    worlds='{"world_id":"00000000-0000-0000-0000-000000000001","name":"existing","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}'
    if test -f "$HOME/created-first"; then
      worlds="$worlds,"' {"world_id":"00000000-0000-0000-0000-000000000002","name":"first","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.3","ssh":{"user":"wt","host":"192.0.2.3","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}'
    fi
    if test -f "$HOME/created-second"; then
      worlds="$worlds,"' {"world_id":"00000000-0000-0000-0000-000000000003","name":"second","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.4","ssh":{"user":"wt","host":"192.0.2.4","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}'
    fi
    printf '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"worlds","worlds":[%s]}}\n' "$worlds"
    ;;
  *'"operation":"create_world"'*)
    name=
    id=
    case "$request" in
      *'"name":"first"'*) name=first; id=00000000-0000-0000-0000-000000000002 ;;
      *'"name":"second"'*) name=second; id=00000000-0000-0000-0000-000000000003 ;;
      *'"name":"capacity"'*) name=capacity ;;
    esac
    if test -n "$name"; then printf '%s\n' "$name" >> "$HOME/create-requests"; fi
    if test "$name" = capacity; then
      while ! test -f "$HOME/release-capacity"; do sleep 0.05; done
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"error","error":{"code":"capacity","message":"full","capacity":{"resource":"cpu","total":2,"reserved":2,"requested":2}}}'
    elif test -f "$HOME/success-creates" && test -n "$name"; then
      printf 'start %s\n' "$name" >> "$HOME/create-log"
      if test "$name" = first; then
        while ! test -f "$HOME/release-first"; do sleep 0.05; done
      else
        sleep 1
      fi
      touch "$HOME/created-$name"
      printf 'end %s\n' "$name" >> "$HOME/create-log"
      printf '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"world","world":{"world_id":"%s","name":"%s","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.3","ssh":{"user":"wt","host":"192.0.2.3","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}\n' "$id" "$name"
    else
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"event":"progress","message":"Waiting for the guest transport..."}'
      sleep 5
      printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"error","error":{"code":"backend","message":"fixture stopped creation"}}'
    fi
    ;;
  *'"operation":"delete"'*)
    sleep 5
    printf '%s\n' '{"protocol_version":@PROTOCOL_VERSION@,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-0000-0000-000000000001"}}'
    ;;
  *) exit 2 ;;
esac
"#
        .replace(
            "@PROTOCOL_VERSION@",
            &wt_control_protocol::PROTOCOL_VERSION.to_string(),
        );
        write_executable(&bin.join("wts"), &server);
        write_executable(
            &bin.join("ssh"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/ssh-args\"\ncontrol=\nprevious=\nfor argument do\n  if test \"$previous\" = -S; then control=$argument; fi\n  previous=$argument\n  target=$argument\ndone\ncase \"$*\" in\n  *'-O check'*) test -e \"$control\" ;;\n  *'/usr/local/bin/wtg codex focus-pane wt-host %1'*) printf 'wt-host:%%1\\n' ;;\n  *'-M -S'*)\n    if test \"$target\" = local.first && test -f \"$HOME/success-creates\"; then\n      while ! test -f \"$HOME/release-first-ssh\"; do sleep 0.05; done\n    fi\n    touch \"$control\"; stty -echo; printf 'session: %s\\n' \"$target\"; exec cat\n    ;;\n  *) exit 2 ;;\nesac\n",
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
        .wait_for_text("No live Codex panes")?
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?
        .click(13, 11)?
        .wait_for_text("Review")?
        .press(Key::Escape)?
        .wait_for_text("No live Codex panes")?
        .wait_for_text_gone("Create world")?;
    Ok(())
}

#[test]
fn two_world_creations_run_one_after_the_other() -> Result<()> {
    let fixture = Fixture::new();
    fs::write(fixture.home.path().join("success-creates"), "").unwrap();
    let mut screen = fixture.screen()?;
    screen.wait_for_text("No live Codex panes")?;

    submit_world(&mut screen, "first")?;
    screen.wait_for_text("Create local.first")?;
    open_world_form(&mut screen)?;
    screen.wait_for_text("Actions")?;
    complete_world_form(&mut screen, "second")?;
    assert_eq!(
        fs::read_to_string(fixture.home.path().join("create-log"))?,
        "start first\n"
    );
    fs::write(fixture.home.path().join("release-first"), "").unwrap();
    let provisioned = "start first\nend first\n";
    let provision_deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < provision_deadline
        && fs::read_to_string(fixture.home.path().join("create-log"))? != provisioned
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        fs::read_to_string(fixture.home.path().join("create-log"))?,
        provisioned
    );
    fs::write(fixture.home.path().join("release-first-ssh"), "").unwrap();
    let expected = "start first\nend first\nstart second\nend second\n";
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline
        && fs::read_to_string(fixture.home.path().join("create-log"))? != expected
    {
        std::thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(
        fs::read_to_string(fixture.home.path().join("create-log"))?,
        expected
    );
    Ok(())
}

#[test]
fn capacity_cancellation_acknowledges_and_clears_the_old_tail() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen.wait_for_text("No live Codex panes")?;

    submit_world(&mut screen, "capacity")?;
    screen.wait_for_text("Create local.capacity")?;
    submit_world(&mut screen, "second")?;
    fs::write(fixture.home.path().join("release-capacity"), "").unwrap();
    screen
        .wait_for_text("World capacity is full")?
        .press(Key::Escape)?
        .wait_for_text("Action failed")?;
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(
        fs::read_to_string(fixture.home.path().join("create-requests"))?,
        "capacity\n"
    );
    Ok(())
}

#[test]
fn world_creation_runs_behind_a_live_progress_notification() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex panes")?
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
        .wait_for_text_gone("Waiting for the guest transport...")?;
    Ok(())
}

#[test]
fn world_creation_progress_can_be_hidden_without_blocking_navigation() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex panes")?
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
        .wait_for_text("No live Codex panes")?;
    Ok(())
}

#[test]
fn world_deletion_progress_can_be_hidden_without_blocking_navigation() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex panes")?
        .press(Key::Function(1))?
        .type_text("delete")?
        .press(Key::Enter)?
        .wait_for_text("Delete world")?
        .press(Key::Enter)?
        .wait_for_text("Delete world?")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text("Deleting world")?
        .wait_for_text("local.existing")?
        .press(Key::Tab)?
        .wait_for_text("Worlds")?
        .click(97, 1)?
        .wait_for_text_gone("Deleting world")?;
    Ok(())
}

#[test]
fn world_card_menu_opens_delete_confirmation_without_the_picker() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("No live Codex panes")?
        .press(Key::Tab)?
        .wait_for_text("… Menu")?
        .click(49, 0)?
        .wait_for_text("World Menu")?
        .wait_for_text("local.existing")?
        .press(Key::Enter)?
        .wait_for_text("Delete world?")?
        .press(Key::Escape)?
        .wait_for_text_gone("Delete world?")?;
    Ok(())
}

#[test]
fn pane_observations_refresh_after_startup() -> Result<()> {
    let fixture = Fixture::new();
    let mut screen = fixture.screen()?;
    screen.wait_for_text("No live Codex panes")?;
    fs::write(fixture.home.path().join("pane-observed"), "").unwrap();
    screen
        .wait_for_text("session: local.existing")?
        .wait_for_quiet(Duration::from_millis(50))?;
    Ok(())
}

#[test]
fn world_cards_correlate_observed_byobu_panes_and_show_idle_worlds() -> Result<()> {
    let fixture = Fixture::new();
    fs::write(fixture.home.path().join("pane-observed"), "").unwrap();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("session: local.existing")?
        .press(Key::Tab)?
        .wait_for_text("IDLE · NO RECENT PANE CHANGE")?
        .wait_for_text("Codex wt-host:%1 · STATIC")?
        .wait_for_text("Git write github.com/owner/write")?
        .wait_for_text("Git read github.com/owner/read")?;
    Ok(())
}

#[test]
fn live_preview_focuses_the_matching_byobu_pane_and_opens_its_world() -> Result<()> {
    let fixture = Fixture::new();
    fs::write(fixture.home.path().join("pane-observed"), "").unwrap();
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("session: local.existing")?
        .click(20, 8)?
        .wait_for_text("F5: dashboard")?
        .wait_for_quiet(Duration::from_millis(50))?;
    let calls = fs::read_to_string(fixture.home.path().join("ssh-args"))?;
    assert_eq!(calls.lines().count(), 3);
    assert!(calls.starts_with("-M -S "));
    assert!(calls.contains("-O check -- local.existing-direct"));
    assert!(
        calls.contains("-- local.existing-direct /usr/local/bin/wtg codex focus-pane wt-host %1")
    );
    Ok(())
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn submit_world(screen: &mut Screen, name: &str) -> Result<()> {
    open_world_form(screen)?;
    complete_world_form(screen, name)
}

fn open_world_form(screen: &mut Screen) -> Result<()> {
    screen
        .press(Key::Function(1))?
        .type_text("new")?
        .press(Key::Enter)?
        .wait_for_text("Create world")?;
    Ok(())
}

fn complete_world_form(screen: &mut Screen, name: &str) -> Result<()> {
    screen
        .press(Key::Tab)?
        .press(Key::Enter)?
        .wait_for_quiet(Duration::from_millis(50))?
        .type_text(name)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("Review")?
        .press(Key::Enter)?;
    Ok(())
}
