use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use wt_control_protocol::{Operation, Response};

#[test]
#[ignore = "requires installed KVM image and host integration"]
fn shell_creates_and_deletes_a_real_world() {
    let _lock = KVM_TEST_LOCK.lock().unwrap();
    let mut timings = Timings::new();
    let harness = KvmHarness::new(&mut timings);
    let name = unique_name("shell");
    prepare_client(&harness);

    let path = std::env::join_paths(
        std::iter::once(harness.config.install.binary_dir.clone()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .unwrap();
    let environment = [
        ("HOME", harness.temp.path().as_os_str().to_os_string()),
        ("PATH", path),
        (
            "WT_AGENT_TOOL_TEST_CONTROL_SOCKET",
            harness
                .temp
                .path()
                .join("gateway-control.sock")
                .into_os_string(),
        ),
    ];
    let mut screen = Screen::launch(
        &harness.wt_binary,
        &["shell"],
        harness.temp.path(),
        &environment,
        Duration::from_secs(10),
    )
    .unwrap();

    screen.wait_for_text("WT E2E TEST SERVER").unwrap();
    create_world_with_defaults(&mut screen, name.as_str()).unwrap();

    let Response::Instances { instances, .. } = call_api(
        harness.temp.path(),
        &harness.server_config_path,
        Operation::List,
    ) else {
        panic!("expected list response");
    };
    assert!(instances.iter().any(|instance| instance.name == name));

    delete_world(&mut screen, name.as_str()).unwrap();
    let Response::Instances { instances, .. } = call_api(
        harness.temp.path(),
        &harness.server_config_path,
        Operation::List,
    ) else {
        panic!("expected list response");
    };
    assert!(instances.iter().all(|instance| instance.name != name));

    screen
        .press(Key::Function(6))
        .unwrap()
        .wait_for_exit(0)
        .unwrap();
}

fn prepare_client(harness: &KvmHarness) {
    fs::create_dir_all(harness.temp.path().join(".wt")).unwrap();
    fs::write(
        harness.temp.path().join(".wt/config.toml"),
        "version = 1\n\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    fs::write(
        harness.temp.path().join(".gitconfig"),
        "[user]\n\tname = WT E2E\n\temail = wt@example.invalid\n",
    )
    .unwrap();
    fs::copy(
        &harness.git.guest_key,
        harness.temp.path().join(".ssh/id_ed25519"),
    )
    .unwrap();
    fs::set_permissions(
        harness.temp.path().join(".ssh/id_ed25519"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::copy(
        &harness.git.guest_public_key,
        harness.temp.path().join(".ssh/id_ed25519.pub"),
    )
    .unwrap();

    let wrapper = harness.config.install.binary_dir.join("wt-server");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec '{}' --config '{}' --capacity '{}' api\n",
            env!("CARGO_BIN_EXE_wt-test-server"),
            harness.server_config_path.display(),
            harness.temp.path().join("capacity.toml").display(),
        ),
    )
    .unwrap();
    fs::set_permissions(wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    let ssh = harness.config.install.binary_dir.join("ssh");
    fs::write(
        &ssh,
        "#!/bin/sh\nexec /usr/bin/ssh -F \"$HOME/.ssh/config\" \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(ssh, fs::Permissions::from_mode(0o755)).unwrap();
}
