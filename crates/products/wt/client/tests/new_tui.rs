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
        fs::create_dir_all(home.path().join(".config/wt")).unwrap();
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
            home.path().join(".config/wt/cloud-init.yaml"),
            "#cloud-config\n",
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

    fn screen(&self, arguments: &[&str]) -> Result<Screen> {
        Screen::launch(
            env!("CARGO_BIN_EXE_wt"),
            arguments,
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

fn local_context() -> &'static str {
    "[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n"
}

fn complete_dev_fields(screen: &mut Screen) -> Result<()> {
    screen
        .press(Key::Enter)?
        .type_text("repo-feature")?
        .press(Key::Enter)?
        .type_text("git@example.test:repo.git")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .press(Key::Enter)?;
    Ok(())
}

#[test]
fn dev_form_is_a_full_screen_terminal_ui() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen(&["new", "dev"])?;

    screen
        .wait_for_text("Create development world")?
        .wait_for_text("Git repository")?
        .wait_for_text("Tab/Shift-Tab focus")?
        .wait_for_quiet(Duration::from_millis(50))?;

    insta::assert_snapshot!(screen.contents());
    Ok(())
}

#[test]
fn enter_validates_before_advancing() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen(&["new", "dev"])?;

    screen
        .wait_for_text("Create development world")?
        .press(Key::Enter)?
        .press(Key::Enter)?
        .wait_for_text("invalid instance name")?
        .type_text("valid-name")?
        .press(Key::Enter)?
        .wait_for_text("git@example.com:team/repository.git")?;
    Ok(())
}

#[test]
fn horizontal_arrows_select_a_context_and_vertical_arrows_move_focus() -> Result<()> {
    let fixture = Fixture::new(
        "[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n\
         [[contexts]]\nname = \"lab\"\nkind = \"bare_metal_local\"\n",
    );
    let mut screen = fixture.screen(&["new", "dev"])?;

    screen
        .wait_for_text("‹ local ›")?
        .press(Key::Right)?
        .wait_for_text("‹ lab ›")?
        .press(Key::Down)?
        .type_text("demo")?
        .press(Key::Up)?
        .press(Key::Left)?
        .wait_for_text("‹ local ›")?;
    Ok(())
}

#[test]
fn completed_fields_open_review_and_b_returns_to_editing() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen(&["new", "dev"])?;

    screen.wait_for_text("Create development world")?;
    complete_dev_fields(&mut screen)?;
    screen
        .wait_for_text("Review")?
        .wait_for_text("Resources   2 CPU · 4096 MiB RAM · 32 GiB disk")?
        .wait_for_quiet(Duration::from_millis(50))?;
    insta::assert_snapshot!(screen.contents());
    screen
        .press(Key::Char('b'))?
        .wait_for_text("Git repository")?
        .wait_for_text_gone("Enter create")?;
    Ok(())
}

#[test]
fn host_form_shows_the_cloud_init_recipe_and_no_repository_fields() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let mut screen = fixture.screen(&["new"])?;

    screen
        .wait_for_text("Create host world")?
        .wait_for_text("cloud-init.yaml")?;
    assert!(!screen.contents().contains("Git repository"));
    Ok(())
}

#[test]
fn escape_cancels_without_contacting_the_server() -> Result<()> {
    let fixture = Fixture::new(local_context());
    let contacted = fixture.home.path().join("server-contacted");
    replace_server_with_contact_marker(fixture.home.path(), &contacted);
    let mut screen = fixture.screen(&["new", "dev"])?;

    screen
        .wait_for_text("Create development world")?
        .press(Key::Escape)?
        .wait_for_exit(1)?;
    assert!(!contacted.exists());
    Ok(())
}

fn replace_server_with_contact_marker(home: &Path, marker: &Path) {
    let server = home.join("bin/wt-server");
    fs::write(
        &server,
        format!("#!/bin/sh\ntouch '{}'\nexit 2\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(server, fs::Permissions::from_mode(0o755)).unwrap();
}
