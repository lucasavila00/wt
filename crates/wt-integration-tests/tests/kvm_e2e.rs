use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{
    ApiRequest, ApiResponse, CreateInstance, InstanceName, InstanceStatus, Operation, Outcome,
    Response,
};
use wt_command::cmd;
use wt_server::ServerConfig;

const FIXTURE_SOURCE: &str = "https://github.com/lucasavila00/small-devcontainer-fixture.git";

#[test]
#[ignore = "requires a configured Ubuntu/KVM host"]
fn local_service_runs_small_devcontainer_fixture() {
    let mut timings = Timings::new();
    let temp = TempDir::new().unwrap();
    let workspace = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    timings.run("build guest helpers", || {
        let mut command = cmd!(env!("CARGO"), "build", "-p", "wt-guest");
        command.current_dir(&workspace);
        run(command, "build guest helpers")
    });
    let mut config = match std::env::var_os("WT_KVM_SERVER_CONFIG") {
        Some(path) => ServerConfig::load_from(Path::new(&path)).unwrap(),
        None => ServerConfig::load().unwrap(),
    };
    config.install.binary_dir = workspace.join("target/debug");
    let bridge_ip = network_address(&config.libvirt.network);
    let git = timings.run("prepare SSH Git fixture", || {
        GitSshServer::start(temp.path(), bridge_ip)
    });
    config.git.known_hosts_file = temp.path().join(".ssh/known_hosts");
    let guest_public_key = fs::read_to_string(&git.guest_public_key)
        .unwrap()
        .trim()
        .to_owned();
    std::env::set_var("HOME", temp.path());
    fs::create_dir_all(temp.path().join(".ssh")).unwrap();
    fs::write(
        temp.path().join(".ssh/config"),
        format!(
            "Include {}\nHost *\n  IdentityFile {}\n  IdentitiesOnly yes\n",
            temp.path().join(".ssh/wt/config").display(),
            git.guest_key.display(),
        ),
    )
    .unwrap();

    let server_config_path = temp.path().join("server.toml");
    fs::write(&server_config_path, toml::to_string(&config).unwrap()).unwrap();
    let name = InstanceName::parse(format!(
        "era15-kvm-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ))
    .unwrap();
    let cache_log_since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let created = timings.run("create KVM devcontainer world", || {
        call_api(
            temp.path(),
            &server_config_path,
            Operation::Create(CreateInstance {
                name: name.clone(),
                source: git.url(),
                git_branch: None,
                git_ref: None,
                git_user_name: "WT E2E".to_owned(),
                git_user_email: "wt@example.invalid".to_owned(),
                vcpus: 1,
                memory_mib: 1024,
                disk_gib: 32,
                ssh_authorized_keys: vec![guest_public_key.clone()],
            }),
        )
    });
    let Response::Instance { instance } = created else {
        panic!("expected instance");
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    let instance = *instance;
    assert!(!instance.ssh.as_ref().unwrap().host_keys.is_empty());
    let peer_name = InstanceName::parse(format!("{}-peer", name.as_str())).unwrap();
    let peer_created = timings.run("create peer KVM world", || {
        call_api(
            temp.path(),
            &server_config_path,
            Operation::Create(CreateInstance {
                name: peer_name.clone(),
                source: git.url(),
                git_branch: None,
                git_ref: None,
                git_user_name: "WT E2E".to_owned(),
                git_user_email: "wt@example.invalid".to_owned(),
                vcpus: 1,
                memory_mib: 1024,
                disk_gib: 32,
                ssh_authorized_keys: vec![guest_public_key.clone()],
            }),
        )
    });
    let Response::Instance {
        instance: peer_instance,
    } = peer_created
    else {
        panic!("expected peer instance");
    };
    assert_eq!(peer_instance.status, InstanceStatus::Setup);
    let peer_instance = *peer_instance;
    assert_ne!(instance.guest_ip, peer_instance.guest_ip);
    assert_ne!(
        instance.ssh.as_ref().unwrap().host_keys,
        peer_instance.ssh.as_ref().unwrap().host_keys
    );

    let Response::Instances { instances } =
        call_api(temp.path(), &server_config_path, Operation::List)
    else {
        panic!("expected list response");
    };
    sync_inventory(&instances).unwrap();
    let agent = SshAgent::start(temp.path(), &git.git_key);
    let mut setup = start_world_setup(temp.path(), &name, &agent);
    let mut peer_setup = start_world_setup(temp.path(), &peer_name, &agent);
    let instance = wait_for_running(temp.path(), &server_config_path, &name);
    let peer_instance = wait_for_running(temp.path(), &server_config_path, &peer_name);
    let _ = setup.kill();
    let _ = setup.wait();
    let _ = peer_setup.kill();
    let _ = peer_setup.wait();
    assert_ne!(
        instance.app_ssh.as_ref().unwrap().host_keys,
        peer_instance.app_ssh.as_ref().unwrap().host_keys
    );
    assert_registry_cache_hit(cache_log_since);

    let result = (|| {
        let Response::Instances { instances } =
            call_api(temp.path(), &server_config_path, Operation::List)
        else {
            return Err("expected list response".to_owned());
        };
        assert_eq!(instances.len(), 2);
        timings.run("sync SSH inventory", || sync_inventory(&instances))?;

        let host_alias = format!("local.{}-host", name.as_str());
        let vs_alias = format!("local.{}-vs", name.as_str());
        let peer_host_alias = format!("local.{}-host", peer_name.as_str());
        let ssh_config = temp.path().join(".ssh/config");
        let output = timings.run("verify guest SSH", || {
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &host_alias,
                "test -d /workspace/.git && test ! -e /etc/sudoers.d/wt-setup && test ! -e /var/lib/wt-setup/source && test ! -e /var/lib/wt-setup/git-known-hosts && test ! -e /var/lib/wt-setup/authorized-keys && test ! -e /var/lib/wt-setup/deferred-packages && test ! -e /var/lib/wt-setup/root-prepared && test \"$(nproc)\" = 1 && memory=$(awk '/MemTotal/ {print $2}' /proc/meminfo) && test \"$memory\" -ge 800000 && test \"$memory\" -le 1100000 && sectors=$(cat /sys/block/vda/size) && test \"$sectors\" -ge 67108864",
            )
            .output()
            .map_err(|error| error.to_string())
        })?;
        ensure_success("enter fixture guest host", &output)?;
        let output = timings.run("verify direct devcontainer SSH", || {
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                &vs_alias,
                "test -d /workspaces/small-devcontainer-fixture && ssh-add -L >/dev/null",
            )
            .env("SSH_AUTH_SOCK", &agent.socket)
            .output()
            .map_err(|error| error.to_string())
        })?;
        ensure_success("enter fixture devcontainer over SSH", &output)?;
        let executable = "/usr/bin/byobu-tmux";
        let output = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            format!("test -x {executable}"),
        )
        .output()
        .map_err(|error| error.to_string())?;
        ensure_success("verify Byobu frontend", &output)?;
        let machine_id = git_output(
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &host_alias,
                "cat",
                "/etc/machine-id",
            ),
            "read fixture machine ID",
        );
        let peer_machine_id = git_output(
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &peer_host_alias,
                "cat",
                "/etc/machine-id",
            ),
            "read peer machine ID",
        );
        if machine_id.trim().is_empty() || peer_machine_id.trim().is_empty() {
            return Err("guest machine ID is empty".to_owned());
        }
        if machine_id.trim() == peer_machine_id.trim() {
            return Err(format!("guest machine IDs are duplicated: {machine_id:?}"));
        }

        let mut persistent = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            name.as_str(),
        )
        .env("SSH_AUTH_SOCK", &agent.socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start persistent app shell: {error}"))?;
        persistent
            .stdin
            .as_mut()
            .unwrap()
            .write_all(
                b"ssh-add -L >/dev/null && export WT_PERSISTENCE_MARKER=retained; cd /tmp; printf '%s\\n' \"$WT_PERSISTENCE_MARKER:$PWD\"\n",
            )
            .map_err(|error| format!("initialize persistent app shell: {error}"))?;
        wait_for_line(&mut persistent, "retained:/tmp")?;
        disconnect(&mut persistent, "initial persistent app shell")?;

        let mut reattached = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            name.as_str(),
        )
        .env("SSH_AUTH_SOCK", &agent.socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("reattach persistent app shell: {error}"))?;
        reattached
            .stdin
            .as_mut()
            .unwrap()
            .write_all(
                b"test \"$WT_PERSISTENCE_MARKER\" = retained && test \"$PWD\" = /tmp && printf 'persistence-%s\\n' \"$WT_PERSISTENCE_MARKER\"\n",
            )
            .map_err(|error| format!("verify persistent app shell: {error}"))?;
        wait_for_line(&mut reattached, "persistence-retained")?;
        disconnect(&mut reattached, "reattached app shell")?;

        let output = cmd!(
            "ssh",
            "-F",
            &ssh_config,
            "-i",
            &git.guest_key,
            &host_alias,
            "/usr/bin/tmux",
            "new-window",
            "\\;",
            "split-window",
            "\\;",
            "list-panes",
            "-a",
            "-F",
            "'#{pane_start_command}'",
        )
        .output()
        .map_err(|error| error.to_string())?;
        ensure_success("create persistent app window and split", &output)?;
        let panes = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        if panes
            != "/usr/local/bin/wt-setup-world\n/usr/local/bin/wt-app-pane\n/usr/local/bin/wt-app-pane\n"
        {
            return Err(format!("unexpected tmux pane commands: {panes:?}"));
        }
        let prefix = git_output(
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &host_alias,
                "/usr/bin/tmux",
                "show-options",
                "-gv",
                "prefix",
            ),
            "read persistent session prefix",
        );
        let expected_prefix = "F12";
        if prefix.trim() != expected_prefix {
            return Err(format!(
                "unexpected Byobu session prefix: {prefix:?}; expected {expected_prefix}"
            ));
        }

        let branch = format!("wt-e2e-{}", std::process::id());
        let app_commands = temp.path().join("app-commands");
        fs::write(
            &app_commands,
            format!(
                "set -eu\ntest -n \"$BASH_VERSION\"\ntest \"$(id -u)\" -eq 0\ntest \"$(pwd)\" = /workspaces/small-devcontainer-fixture\ntest \"$(git config user.name)\" = 'WT E2E'\ntest \"$(git config user.email)\" = wt@example.invalid\ngit switch -c {branch}\nprintf 'committed\\n' > wt-e2e.txt\ngit add wt-e2e.txt\ngit commit -m wt-e2e\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        let input = fs::File::open(&app_commands).map_err(|error| error.to_string())?;
        let output = timings.run("commit from app container", || {
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                &vs_alias,
                "cd /workspaces/small-devcontainer-fixture && exec /bin/bash",
            )
            .stdin(Stdio::from(input))
            .output()
            .map_err(|error| error.to_string())
        })?;
        ensure_success("commit from fixture app container", &output)?;
        Ok(())
    })();

    let peer_removed = timings.run("remove peer KVM world", || {
        call_api_result(
            temp.path(),
            &server_config_path,
            Operation::Delete { name: peer_name },
        )
    });
    assert!(
        peer_removed.is_ok(),
        "remove peer KVM sample world: {peer_removed:?}"
    );
    let removed = timings.run("remove KVM world", || {
        call_api_result(temp.path(), &server_config_path, Operation::Delete { name })
    });
    assert!(removed.is_ok(), "remove KVM sample world: {removed:?}");
    result.unwrap();
}

fn wait_for_running(home: &Path, config: &Path, name: &InstanceName) -> wt_api::Instance {
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        let Response::Instance { instance } =
            call_api(home, config, Operation::Get { name: name.clone() })
        else {
            panic!("expected instance response")
        };
        if instance.status == InstanceStatus::Running {
            return *instance;
        }
        assert_ne!(
            instance.status,
            InstanceStatus::Error,
            "setup failed: {instance:?}"
        );
        assert!(Instant::now() < deadline, "timed out waiting for setup");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn sync_inventory(instances: &[wt_api::Instance]) -> Result<(), String> {
    let client_config = wt_cli::config::ClientConfig {
        contexts: vec![wt_cli::config::Context {
            name: "local".into(),
            kind: wt_cli::config::ContextKind::BareMetalLocal,
        }],
    };
    wt_cli::ssh::sync(
        &client_config,
        &instances
            .iter()
            .cloned()
            .map(|instance| wt_cli::inventory::ContextInstance {
                context: "local".into(),
                instance,
            })
            .collect::<Vec<_>>(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn start_world_setup(home: &Path, name: &InstanceName, agent: &SshAgent) -> Child {
    cmd!(
        "ssh",
        "-F",
        home.join(".ssh/config"),
        format!("local.{name}")
    )
    .env("SSH_AUTH_SOCK", &agent.socket)
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()
    .expect("start first-SSH world setup")
}

struct SshAgent {
    child: Child,
    socket: String,
}

impl SshAgent {
    fn start(root: &Path, identity: &Path) -> Self {
        let child = cmd!("ssh-agent", "-D", "-a", root.join("agent.sock"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = root.join("agent.sock").display().to_string();
        for _ in 0..50 {
            if Path::new(&socket).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let askpass = root.join("askpass.sh");
        fs::write(&askpass, "#!/bin/sh\nprintf '%s\\n' secret\n").unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&askpass, fs::Permissions::from_mode(0o700)).unwrap();
        let output = cmd!("ssh-add", identity)
            .env("SSH_AUTH_SOCK", &socket)
            .env("SSH_ASKPASS", &askpass)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", ":0")
            .output()
            .unwrap();
        ensure_success("add Git identity to test agent", &output).unwrap();
        Self { child, socket }
    }
}

impl Drop for SshAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn call_api(home: &Path, config: &Path, operation: Operation) -> Response {
    call_api_result(home, config, operation).unwrap()
}

fn call_api_result(home: &Path, config: &Path, operation: Operation) -> Result<Response, String> {
    // Match the restrictive umask of the installed wt-server.service. QEMU must
    // still be able to traverse the world directory and open its disk images.
    let mut child = cmd!(
        "/bin/sh",
        "-c",
        "umask 077; exec \"$@\"",
        "sh",
        env!("CARGO_BIN_EXE_wt-test-server"),
        "--config",
        config,
        "api",
    )
    .env("HOME", home)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| error.to_string())?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .ok_or("test server stdin unavailable")?,
        &ApiRequest::new(operation),
    )
    .map_err(|error| error.to_string())?;
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    ensure_success("call test server API", &output)?;
    let response: ApiResponse =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    match response.outcome {
        Outcome::Ok { response } => Ok(*response),
        Outcome::Error { error } => Err(format!("{}: {}", error.code as u8, error.message)),
    }
}

fn assert_registry_cache_hit(since: u64) {
    let output = cmd!(
        "docker",
        "logs",
        "--since",
        since.to_string(),
        "wt-registry-cache",
    )
    .output()
    .expect("read registry cache logs");
    assert!(
        output.status.success(),
        "read registry cache logs: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let has_hit = String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|value| value["upstream_cache_status"].as_str() == Some("HIT"));
    assert!(
        has_hit,
        "registry cache recorded no HIT during world creation"
    );
}

struct GitSshServer {
    child: Child,
    address: IpAddr,
    port: u16,
    repository: PathBuf,
    git_key: PathBuf,
    guest_key: PathBuf,
    guest_public_key: PathBuf,
}

impl GitSshServer {
    fn start(root: &Path, address: IpAddr) -> Self {
        let repository = root.join("small-devcontainer-fixture.git");
        run(
            cmd!("git", "clone", "--bare", FIXTURE_SOURCE, &repository),
            "create bare fixture repository",
        );
        let git_key = root.join("git-client");
        let guest_key = root.join("guest-client");
        let host_key = root.join("ssh-host");
        generate_key(&git_key, "secret");
        generate_key(&guest_key, "");
        generate_key(&host_key, "");
        let git_public_key = git_key.with_extension("pub");
        let guest_public_key = guest_key.with_extension("pub");
        let authorized_keys = root.join("authorized_keys");
        fs::copy(&git_public_key, &authorized_keys).unwrap();

        let listener = TcpListener::bind((address, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let config = root.join("sshd_config");
        fs::write(
            &config,
            format!(
                "Port {port}\nListenAddress {address}\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nUsePAM no\nPermitRootLogin no\nStrictModes no\nAllowUsers {}\nLogLevel ERROR\n",
                host_key.display(),
                root.join("sshd.pid").display(),
                authorized_keys.display(),
                current_user(),
            ),
        )
        .unwrap();
        let mut child = cmd!("/usr/sbin/sshd", "-D", "-e", "-f", &config)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start temporary SSH Git server; install openssh-server on the host");
        for _ in 0..50 {
            if TcpStream::connect((address, port)).is_ok() {
                let host_public = fs::read_to_string(host_key.with_extension("pub")).unwrap();
                let mut fields = host_public.split_whitespace();
                let kind = fields.next().unwrap();
                let data = fields.next().unwrap();
                let ssh = root.join(".ssh");
                fs::create_dir(&ssh).unwrap();
                fs::write(
                    ssh.join("known_hosts"),
                    format!("[{address}]:{port} {kind} {data}\n"),
                )
                .unwrap();
                return Self {
                    child,
                    address,
                    port,
                    repository,
                    git_key,
                    guest_key,
                    guest_public_key,
                };
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("temporary SSH Git server did not become ready");
    }

    fn url(&self) -> String {
        format!(
            "ssh://{}@{}:{}/{}",
            current_user(),
            self.address,
            self.port,
            self.repository.display()
        )
    }
}

struct Timings {
    started: Instant,
    phases: Vec<(&'static str, Duration)>,
}

impl Timings {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            phases: Vec::new(),
        }
    }

    fn run<T>(&mut self, label: &'static str, action: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = action();
        self.phases.push((label, started.elapsed()));
        result
    }
}

impl Drop for Timings {
    fn drop(&mut self) {
        eprintln!("KVM E2E timings:");
        for (label, elapsed) in &self.phases {
            eprintln!("  {label}: {:.1}s", elapsed.as_secs_f64());
        }
        eprintln!("  total: {:.1}s", self.started.elapsed().as_secs_f64());
    }
}

impl Drop for GitSshServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn generate_key(path: &Path, passphrase: &str) {
    run(
        cmd!(
            "ssh-keygen",
            "-q",
            "-t",
            "ed25519",
            "-N",
            passphrase,
            "-f",
            path,
        ),
        "generate test SSH key",
    );
}

fn current_user() -> String {
    std::env::var("USER").expect("USER is set")
}

fn network_address(network: &str) -> IpAddr {
    let output = cmd!(
        "virsh",
        "-c",
        wt_libvirt::LIBVIRT_URI,
        "net-dumpxml",
        network,
    )
    .output()
    .unwrap();
    ensure_success("inspect libvirt network", &output).unwrap();
    let xml = String::from_utf8(output.stdout).unwrap();
    for quote in ['\'', '"'] {
        let needle = format!("<ip address={quote}");
        if let Some(rest) = xml.split_once(&needle).map(|(_, rest)| rest) {
            if let Some(value) = rest.split_once(quote).map(|(value, _)| value) {
                return value.parse().unwrap();
            }
        }
    }
    panic!("configured libvirt network has no host address");
}

fn git_output(mut command: Command, action: &str) -> String {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn wait_for_line(child: &mut Child, expected: &str) -> Result<(), String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "SSH child stdout is not piped".to_owned())?;
    let expected = expected.to_owned();
    let reader_expected = expected.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut output = BufReader::new(stdout);
        let mut line = String::new();
        let mut found = false;
        loop {
            line.clear();
            match output.read_line(&mut line) {
                Ok(0) if !found => {
                    let _ = sender.send(Err(format!(
                        "app shell closed before printing {reader_expected:?}"
                    )));
                    return;
                }
                Ok(0) => return,
                Ok(_) if !found && line.contains(&reader_expected) => {
                    let _ = sender.send(Ok(()));
                    found = true;
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = sender.send(Err(format!("read app shell output: {error}")));
                    return;
                }
            }
        }
    });
    match receiver.recv_timeout(Duration::from_secs(20)) {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("app shell did not print {expected:?} within 20s"))
        }
    }
}

fn disconnect(child: &mut Child, description: &str) -> Result<(), String> {
    child
        .kill()
        .map_err(|error| format!("disconnect {description}: {error}"))?;
    child
        .wait()
        .map_err(|error| format!("wait for disconnected {description}: {error}"))?;
    Ok(())
}

fn run(mut command: Command, action: &str) {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
}

fn ensure_success(action: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{action} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
