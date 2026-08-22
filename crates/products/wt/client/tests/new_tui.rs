mod support;

use anyhow::Result;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use support::{Key, Screen};

struct Fixture {
    home: tempfile::TempDir,
    path: OsString,
}

impl Fixture {
    fn new(contexts: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".wt")).unwrap();
        fs::create_dir_all(home.path().join(".ssh")).unwrap();
        fs::write(
            home.path().join(".wt/config.toml"),
            format!("version = 1\n{contexts}"),
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
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let server = bin.join("wt-server");
        fs::write(&server, "#!/bin/sh\nexit 2\n").unwrap();
        fs::set_permissions(&server, fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap();
        Self { home, path }
    }

    fn screen(&self) -> Result<Screen> {
        Screen::launch(
            env!("CARGO_BIN_EXE_wt"),
            &["new"],
            self.home.path(),
            &[
                ("HOME", self.home.path().as_os_str().to_os_string()),
                ("PATH", self.path.clone()),
            ],
            Duration::from_secs(10),
        )
    }
}

fn local_context() -> &'static str {
    "[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n"
}

fn complete_fields(screen: &mut Screen) -> Result<()> {
    screen
        .press(Key::Enter)?
        .type_text("repo-feature")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?;
    Ok(())
}

#[test]
fn world_form_is_a_full_screen_terminal_ui() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("Create world")?
        .wait_for_text("World name")?
        .wait_for_text("Tab/Shift-Tab focus")?
        .wait_for_quiet(Duration::from_millis(50))?;
    insta::assert_snapshot!(screen.contents());
    Ok(())
}

#[test]
fn enter_validates_before_advancing() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("Create world")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("invalid instance name")?;
    Ok(())
}

#[test]
fn completed_fields_open_review_and_b_returns_to_editing() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen()?;
    screen.wait_for_text("Create world")?;
    complete_fields(&mut screen)?;
    screen
        .wait_for_text("Review")?
        .wait_for_text("Resources   2 CPU · 4096 MiB RAM · 32 GiB disk")?
        .press(Key::Char('b'))?
        .wait_for_text("World name")?
        .wait_for_text_gone("Enter create")?;
    Ok(())
}

#[test]
fn escape_cancels_after_server_info_without_creating_a_world() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let requests = fixture.home.path().join("server-requests");
    replace_server_with_request_log(fixture.home.path(), &requests);
    let mut screen = fixture.screen()?;
    screen
        .wait_for_text("Create world")?
        .press(Key::Escape)?
        .wait_for_exit(1)?;
    let requests = fs::read_to_string(requests).unwrap();
    assert!(requests.contains(r#""operation":"server_info""#));
    assert!(!requests.contains(r#""operation":"create""#));
    Ok(())
}

fn replace_server_with_request_log(home: &Path, requests: &Path) {
    let server = home.join("bin/wt-server");
    fs::write(
        &server,
        format!(
            "#!/bin/sh\nrequest=$(cat)\nprintf '%s\\n' \"$request\" >> '{}'\nprintf '%s\\n' '{{\"protocol_version\":9,\"outcome\":\"ok\",\"response\":{{\"response\":\"server_info\",\"test_server\":false}}}}'\n",
            requests.display()
        ),
    )
    .unwrap();
    fs::set_permissions(server, fs::Permissions::from_mode(0o755)).unwrap();
}
