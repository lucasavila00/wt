use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use wt_command::cmd;

#[test]
fn new_requires_a_terminal_before_contacting_server() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let helper = bin.join("wt-server");
    fs::write(
        &helper,
        "#!/bin/sh\ntouch \"$HOME/server-contacted\"\nexit 2\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let output = cmd!(env!("CARGO_BIN_EXE_wt"), "new")
        .env("HOME", temp.path())
        .env("PATH", path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!temp.path().join("server-contacted").exists());
    insta::assert_snapshot!(String::from_utf8_lossy(&output.stderr), @"wt: `wt new` requires an interactive terminal\n");
}

#[test]
fn new_and_rm_always_sync_ssh_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let helper = bin.join("wt-server");
    fs::write(
        &helper,
        r#"#!/bin/sh
set -eu
request=$(cat)
state="$HOME/helper-state"
case "$request" in
  *'"operation":"create"'*)
    attempts="$HOME/helper-attempts"
    count=0
    test ! -f "$attempts" || count=$(cat "$attempts")
    count=$((count + 1))
    printf '%s\n' "$count" > "$attempts"
    case "$request" in
      *'"git_user_name":"Lucas Ávila"'*'"git_user_email":"lucaxx@gmail.com"'*)
        : > "$state"
        printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"repo-feature/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
        ;;
      *) exit 2 ;;
    esac
    ;;
  *'"operation":"get"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"repo-feature/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}}}'
    ;;
  *'"operation":"delete"'*)
    rm -f "$state"
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"deleted","name":"repo-feature"}}'
    ;;
  *'"operation":"list"'*)
    if [ -f "$state" ]; then
      printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"repo-feature","owner":"tester","status":"running","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"repo-feature/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]},"app_ssh":{"user":"vscode","port":2222,"host_keys":["ssh-ed25519 AAAAAPPLICATION app"]}}]}}'
    else
      printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[]}}'
    fi
    ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let ssh = bin.join("ssh");
    fs::write(
        &ssh,
        "#!/bin/sh\nprintf 'ssh exec: %s\\n' \"$*\"\nexit 23\n",
    )
    .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir(temp.path().join(".wt")).unwrap();
    fs::write(
        temp.path().join(".gitconfig"),
        "[user]\n\tname = Lucas Ávila\n\temail = lucaxx@gmail.com\n",
    )
    .unwrap();
    fs::create_dir(temp.path().join(".ssh")).unwrap();
    let key = temp.path().join(".ssh/id_ed25519");
    let generated = cmd!("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", &key)
        .output()
        .unwrap();
    assert!(generated.status.success());
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let mut created = cmd!(
        "script",
        "-qfec",
        format!("{} new", env!("CARGO_BIN_EXE_wt")),
        "/dev/null"
    )
    .env("HOME", temp.path())
    .env("PATH", &path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    created
        .stdin
        .take()
        .unwrap()
        .write_all(b"repo-feature\ngit@example.test:repo.git\nmain\n\n\n\n\n")
        .unwrap();
    let created = created.wait_with_output().unwrap();
    assert_eq!(created.status.code(), Some(23));
    let transcript = String::from_utf8_lossy(&created.stdout).replace('\r', "");
    let completed = transcript
        .find("local.repo-feature\tsetup")
        .map(|start| &transcript[start..])
        .expect("creation result is present in the terminal transcript");
    insta::assert_snapshot!(
        completed,
        @r###"
        local.repo-feature	setup	192.0.2.2

        Starting setup: ssh local.repo-feature
        Guest host: ssh local.repo-feature-host
        Endpoint: wt@192.0.2.2:22
        ssh exec: local.repo-feature
        "###
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("helper-attempts")).unwrap(),
        "1\n"
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    insta::assert_snapshot!(
        "automatically_synced_ssh_config",
        managed.replace(&temp.path().display().to_string(), "[HOME]")
    );
    assert_eq!(
        fs::read_to_string(temp.path().join(".ssh/config")).unwrap(),
        "Include ~/.ssh/wt/config\n"
    );
    let evaluated = cmd!(
        "/usr/bin/ssh",
        "-G",
        "-F",
        temp.path().join(".ssh/config"),
        "local.repo-feature"
    )
    .env("HOME", temp.path())
    .output()
    .unwrap();
    assert!(
        evaluated.status.success(),
        "{}",
        String::from_utf8_lossy(&evaluated.stderr)
    );
    let evaluated = String::from_utf8_lossy(&evaluated.stdout);
    for expected in [
        "hostname 192.0.2.2",
        "user wt",
        "port 22",
        "hostkeyalias local.repo-feature-host",
    ] {
        assert!(evaluated.lines().any(|line| line == expected), "{expected}");
    }

    let removed = cmd!(env!("CARGO_BIN_EXE_wt"), "rm", "repo-feature")
        .env("HOME", temp.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let managed = fs::read_to_string(temp.path().join(".ssh/wt/config")).unwrap();
    assert!(!managed.contains("Host repo-feature"));
}

#[test]
fn new_reports_created_world_when_ssh_config_cannot_be_updated() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(temp.path().join(".wt")).unwrap();
    fs::create_dir_all(temp.path().join(".ssh/config")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".gitconfig"),
        "[user]\n\tname = Test User\n\temail = test@example.test\n",
    )
    .unwrap();
    let key = temp.path().join(".ssh/id_ed25519");
    assert!(
        cmd!("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", &key)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        bin.join("wt-server"),
        r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"operation":"create"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"broken-config","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"broken-config/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
    ;;
  *'"operation":"list"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"broken-config","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"broken-config/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}]}}'
    ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(bin.join("wt-server"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(bin.join("ssh"), "#!/bin/sh\ntouch \"$HOME/ssh-execed\"\n").unwrap();
    fs::set_permissions(bin.join("ssh"), fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let mut created = cmd!(
        "script",
        "-qfec",
        format!("{} new", env!("CARGO_BIN_EXE_wt")),
        "/dev/null"
    )
    .env("HOME", temp.path())
    .env("PATH", path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    created
        .stdin
        .take()
        .unwrap()
        .write_all(b"broken-config\ngit@example.test:repo.git\nmain\n\n\n\n\n")
        .unwrap();
    let created = created.wait_with_output().unwrap();

    assert!(!created.status.success());
    assert!(!temp.path().join("ssh-execed").exists());
    let transcript = String::from_utf8_lossy(&created.stdout)
        .replace('\r', "")
        .replace(&temp.path().display().to_string(), "[HOME]");
    let error = transcript
        .find("wt: world")
        .map(|start| &transcript[start..])
        .expect("failure is present in the terminal transcript");
    insta::assert_snapshot!(error, @r###"
    wt: world local.broken-config was created, but setup was not entered
    resolve the synchronization error, run `wt sync`, and reconnect with `ssh local.broken-config`: configure WT SSH aliases in [HOME]/.ssh/config
    add `Include ~/.ssh/wt/config` before other active directives, then run `wt sync`: [HOME]/.ssh/config is not a regular file
    "###);
}

#[test]
fn new_does_not_exec_ssh_when_another_context_prevents_inventory_sync() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(temp.path().join(".wt")).unwrap();
    fs::create_dir_all(temp.path().join(".ssh")).unwrap();
    fs::write(
        temp.path().join(".wt/config.toml"),
        "version = 1\n[[contexts]]\nname = \"local\"\nkind = \"bare_metal_local\"\n[[contexts]]\nname = \"offline\"\nkind = \"bare_metal_ssh\"\nhost = \"offline-server\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".gitconfig"),
        "[user]\n\tname = Test User\n\temail = test@example.test\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".ssh/config"),
        "Include ~/.ssh/wt/config\n",
    )
    .unwrap();
    let key = temp.path().join(".ssh/id_ed25519");
    assert!(
        cmd!("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", &key)
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        bin.join("wt-server"),
        r#"#!/bin/sh
set -eu
request=$(cat)
case "$request" in
  *'"operation":"create"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instance","instance":{"id":"00000000-0000-0000-0000-000000000001","name":"broken-config","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"broken-config/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}}}'
    ;;
  *'"operation":"list"'*)
    printf '%s\n' '{"protocol_version":1,"outcome":"ok","response":{"response":"instances","instances":[{"id":"00000000-0000-0000-0000-000000000001","name":"broken-config","owner":"tester","status":"setup","kind":"devcontainer","source":"git@example.test:repo.git","git_base":"main","git_prefix":"broken-config/","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"]}}]}}'
    ;;
  *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(bin.join("wt-server"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        bin.join("ssh"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/ssh-calls\"\nexit 2\n",
    )
    .unwrap();
    fs::set_permissions(bin.join("ssh"), fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let mut created = cmd!(
        "script",
        "-qfec",
        format!("{} new", env!("CARGO_BIN_EXE_wt")),
        "/dev/null"
    )
    .env("HOME", temp.path())
    .env("PATH", path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    created
        .stdin
        .take()
        .unwrap()
        .write_all(b"\nbroken-config\ngit@example.test:repo.git\nmain\n\n\n\n\n")
        .unwrap();
    let created = created.wait_with_output().unwrap();

    assert!(!created.status.success());
    assert_eq!(
        fs::read_to_string(temp.path().join("ssh-calls")).unwrap(),
        "-- offline-server wt-server api\n"
    );
    let transcript = String::from_utf8_lossy(&created.stdout).replace('\r', "");
    let error = transcript
        .find("wt: world")
        .map(|start| &transcript[start..])
        .expect("failure is present in the terminal transcript");
    insta::assert_snapshot!(error, @r###"
    wt: world local.broken-config was created, but setup was not entered
    resolve the synchronization error, run `wt sync`, and reconnect with `ssh local.broken-config`: SSH inventory was not updated because the complete world list is unavailable

    error: context offline could not be queried: context helper exited with exit status: 2
      hint: check `ssh offline-server` and `ssh offline-server systemctl status wt-server.service`
    "###);
}
