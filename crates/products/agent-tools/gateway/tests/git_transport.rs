use diesel::prelude::*;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;
use wt_workload_registry::schema::worlds;

struct Process(Option<Child>);

impl Process {
    fn stop(mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn world_scoped_transports_read_and_write_multiple_repositories() {
    let temp = tempfile::tempdir().unwrap();
    let repositories = temp.path().join("repositories");
    let upstream = repositories.join("project.git");
    let other_upstream = repositories.join("other.git");
    fs::create_dir(&repositories).unwrap();
    git(temp.path(), &["init", "--bare", upstream.to_str().unwrap()]);
    git(
        temp.path(),
        &["init", "--bare", other_upstream.to_str().unwrap()],
    );

    let seed = temp.path().join("seed");
    fs::create_dir(&seed).unwrap();
    git(&seed, &["init"]);
    git(&seed, &["config", "user.name", "Test User"]);
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    fs::write(seed.join("README.md"), "base\n").unwrap();
    git(&seed, &["add", "README.md"]);
    git(&seed, &["commit", "-m", "base"]);
    git(&seed, &["branch", "-M", "main"]);
    git(&seed, &["push", upstream.to_str().unwrap(), "main:main"]);
    git(
        &seed,
        &["push", other_upstream.to_str().unwrap(), "main:main"],
    );
    git(
        &seed,
        &[
            "push",
            upstream.to_str().unwrap(),
            "main:refs/heads/wt/existing",
        ],
    );
    git(&upstream, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(
        &other_upstream,
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );

    let control = temp.path().join("control.sock");
    let transport = temp.path().join("transport.sock");
    let second_transport = temp.path().join("second-transport.sock");
    let relay_socket = temp.path().join("relay.sock");
    let second_relay_socket = temp.path().join("second-relay.sock");
    let database = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database).unwrap();
    let first_world = Uuid::new_v4();
    let second_world = Uuid::new_v4();
    insert_world(&registry, first_world, "first");
    insert_world(&registry, second_world, "second");
    let mut gateway = spawn_gateway(
        &control,
        &[(first_world, &transport), (second_world, &second_transport)],
        &database,
        &repositories,
    );
    wait_for(&control);
    wait_for(&transport);

    let _relay = spawn_relay(&relay_socket, &transport);
    let _second_relay = spawn_relay(&second_relay_socket, &second_transport);
    wait_for(&relay_socket);
    wait_for(&second_relay_socket);

    let checkout = temp.path().join("checkout");
    let output = git_output(
        temp.path(),
        &[
            "clone",
            "wt-agent::git@local.test:project.git",
            checkout.to_str().unwrap(),
        ],
        &relay_socket,
    );
    assert_success(&output);
    git(&checkout, &["config", "user.name", "Test User"]);
    git(&checkout, &["config", "user.email", "test@example.invalid"]);

    let other_checkout = temp.path().join("other-checkout");
    assert_success(&git_output(
        temp.path(),
        &[
            "clone",
            "wt-agent::git@local.test:other.git",
            other_checkout.to_str().unwrap(),
        ],
        &relay_socket,
    ));
    git(&other_checkout, &["config", "user.name", "Test User"]);
    git(
        &other_checkout,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&other_checkout, &["switch", "-c", "wt/other"]);
    fs::write(other_checkout.join("OTHER.md"), "other\n").unwrap();
    git(&other_checkout, &["add", "OTHER.md"]);
    git(&other_checkout, &["commit", "-m", "other"]);
    assert_success(&git_output(
        &other_checkout,
        &["push", "origin", "wt/other"],
        &relay_socket,
    ));
    assert_ref(&other_upstream, "refs/heads/wt/other", true);

    fs::write(seed.join("README.md"), "upstream\n").unwrap();
    git(&seed, &["commit", "-am", "upstream"]);
    git(&seed, &["push", upstream.to_str().unwrap(), "main:main"]);
    assert_success(&git_output(&checkout, &["fetch", "origin"], &relay_socket));
    assert_eq!(
        git_stdout(&seed, &["rev-parse", "main"]),
        git_stdout(&checkout, &["rev-parse", "origin/main"])
    );

    git(&checkout, &["switch", "-c", "wt/fix"]);
    fs::write(checkout.join("README.md"), "first\n").unwrap();
    git(&checkout, &["commit", "-am", "first"]);
    let published = git_output(
        &checkout,
        &["push", "-u", "origin", "wt/fix"],
        &relay_socket,
    );
    assert_success(&published);
    let diagnostics = String::from_utf8_lossy(&published.stderr);
    assert!(diagnostics.contains("This is a WT-managed development environment"));
    assert!(diagnostics.contains("Published branch `wt/fix`"));
    assert!(diagnostics.contains("wtg tools --help"));
    assert_ref(&upstream, "refs/heads/wt/fix", true);

    fs::write(checkout.join("README.md"), "second\n").unwrap();
    git(&checkout, &["commit", "-am", "second"]);
    assert_success(&git_output(
        &checkout,
        &["push", "origin", "wt/fix"],
        &second_relay_socket,
    ));
    git(&checkout, &["reset", "--hard", "HEAD^"]);
    assert_success(&git_output(
        &checkout,
        &["push", "--force", "origin", "wt/fix"],
        &second_relay_socket,
    ));
    let local = git_stdout(&checkout, &["rev-parse", "wt/fix"]);
    let published = git_stdout(&upstream, &["rev-parse", "refs/heads/wt/fix"]);
    assert_eq!(local, published);

    git(&checkout, &["switch", "-c", "wrong"]);
    fs::write(checkout.join("README.md"), "wrong\n").unwrap();
    git(&checkout, &["commit", "-am", "wrong"]);
    let rejected = git_output(&checkout, &["push", "origin", "wrong"], &relay_socket);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("must use the shared `wt/` prefix"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert_ref(&upstream, "refs/heads/wrong", false);

    git(&checkout, &["tag", "v1"]);
    let rejected = git_output(&checkout, &["push", "origin", "v1"], &relay_socket);
    assert!(!rejected.status.success());
    assert_ref(&upstream, "refs/tags/v1", false);

    let deleted = git_output(
        &checkout,
        &["push", "origin", "--delete", "wt/fix"],
        &second_relay_socket,
    );
    assert_success(&deleted);
    assert!(String::from_utf8_lossy(&deleted.stderr).contains("Deleted branch `wt/fix`"));
    assert_ref(&upstream, "refs/heads/wt/fix", false);

    let updates = wait_for_branch_updates(&registry, 4);
    assert_eq!(updates[0].world_id, second_world.into());
    assert_eq!(
        updates[0].new_oid.as_deref(),
        Some("0000000000000000000000000000000000000000")
    );
    assert_eq!(updates[3].world_id, first_world.into());
    assert_eq!(
        updates[3].previous_oid.as_deref(),
        Some("0000000000000000000000000000000000000000")
    );

    gateway.stop();
    gateway = spawn_gateway(
        &control,
        &[(first_world, &transport), (second_world, &second_transport)],
        &database,
        &repositories,
    );
    assert_success(&git_output(&checkout, &["fetch", "origin"], &relay_socket));
    assert_success(&git_output(
        &checkout,
        &["fetch", "origin"],
        &second_relay_socket,
    ));
    drop(gateway);
}

fn spawn_gateway(
    control: &Path,
    transports: &[(Uuid, &Path)],
    database: &Path,
    repositories: &Path,
) -> Process {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wt-agent-tool-gateway"));
    command.args([
        "--control-socket",
        control.to_str().unwrap(),
        "--database-path",
        database.to_str().unwrap(),
        "--local-provider",
        &format!("local.test={}", repositories.display()),
    ]);
    for (world_id, path) in transports {
        command.args([
            "--transport-socket",
            &format!("{world_id}={}", path.display()),
        ]);
    }
    let child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for_listener(control);
    for (_, path) in transports {
        wait_for_listener(path);
    }
    Process(Some(child))
}

fn insert_world(registry: &wt_workload_registry::Registry, id: Uuid, name: &str) {
    registry
        .transaction::<_, wt_workload_registry::RegistryError>(|connection| {
            diesel::insert_into(worlds::table)
                .values((
                    worlds::world_id.eq(id.to_string()),
                    worlds::vcpus.eq(1_i64),
                    worlds::memory_mib.eq(1024_i64),
                    worlds::disk_gib.eq(10_i64),
                    worlds::compute_reserved.eq(true),
                    worlds::disk_reserved_gib.eq(10_i64),
                    worlds::owner.eq("alice"),
                    worlds::name.eq(name),
                    worlds::status.eq("running"),
                    worlds::setup_fingerprint.eq("fingerprint"),
                    worlds::ssh_host_keys.eq("[]"),
                ))
                .execute(connection)?;
            Ok(())
        })
        .unwrap();
}

fn spawn_relay(socket: &Path, transport: &Path) -> Process {
    let relay = Command::new(env!("CARGO_BIN_EXE_wt-agent-tool-gateway-relay"))
        .args([
            "--socket",
            socket.to_str().unwrap(),
            "--gateway-unix",
            transport.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Process(Some(relay))
}

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert_success(&output);
}

fn git_output(directory: &Path, args: &[&str], socket: &Path) -> Output {
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_git-remote-wt-agent"));
    let path = format!(
        "{}:{}",
        helper.parent().unwrap().display(),
        std::env::var("PATH").unwrap()
    );
    Command::new("git")
        .current_dir(directory)
        .args(args)
        .env("PATH", path)
        .env("WT_AGENT_TOOL_TEST_SOCKET", socket)
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_ref(repository: &Path, reference: &str, exists: bool) {
    let output = Command::new("git")
        .args([
            "--git-dir",
            repository.to_str().unwrap(),
            "show-ref",
            "--verify",
            reference,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.success(), exists, "{reference}");
}

fn wait_for_branch_updates(
    registry: &wt_workload_registry::Registry,
    expected_count: usize,
) -> Vec<wt_workload_registry::GitActivity> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let updates = registry
            .list_git_activity(
                "alice",
                wt_workload_registry::GitActivityQuery::Branch {
                    provider_host: "local.test".into(),
                    repository: "project".into(),
                    branch: "wt/fix".into(),
                    before_id: None,
                },
            )
            .unwrap();
        if updates.len() == expected_count {
            return updates;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected_count} branch updates; found {}",
            updates.len()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn git_stdout(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert_success(&output);
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_listener(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while UnixStream::connect(path).is_err() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
