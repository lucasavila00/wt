use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{CreateInstance, GitPassphrase, InstanceName, InstanceStatus, Operation, Response};
use wt_command::cmd;
use wt_libvirt::{LibvirtWorker, ServerConfig};
use wt_server::service::Service;
use wt_server::store::Store;

const SAMPLE_SOURCE: &str = "git@github.com:lucasavila00/jsdev-sample.git";
const FIXTURE_IMAGES: &str = include_str!("../fixture-images.txt");

#[test]
fn local_service_runs_and_pushes_from_jsdev_devcontainer() {
    let mut timings = Timings::new();
    let temp = TempDir::new().unwrap();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    timings.run("build guest helpers", || {
        let mut command = cmd!(env!("CARGO"), "build", "-p", "wt-guest");
        command.current_dir(&workspace);
        run(command, "build guest helpers")
    });
    let mut config = ServerConfig::load().unwrap();
    assert_eq!(config.registry_cache.preload_images, fixture_images());
    config.install.binary_dir = workspace.join("target/debug");
    let bridge_ip = network_address(&config.libvirt.network);
    let git = timings.run("prepare SSH Git fixture", || {
        GitSshServer::start(temp.path(), bridge_ip)
    });
    config.guest.ssh_authorized_keys_file = git.guest_public_key.clone();
    config.git.identity_file = git.git_key.clone();
    config.git.known_hosts_file = temp.path().join(".ssh/known_hosts");
    std::env::set_var("HOME", temp.path());
    fs::create_dir_all(temp.path().join(".ssh")).unwrap();
    fs::write(
        temp.path().join(".ssh/config"),
        format!("Include {}\n", temp.path().join(".ssh/wt/config").display()),
    )
    .unwrap();

    let worker = LibvirtWorker::new(config.worker_config().unwrap()).unwrap();
    let mut service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
    );
    let name = InstanceName::parse(format!(
        "era15-kvm-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ))
    .unwrap();
    let created = timings.run("create KVM devcontainer world", || {
        service
            .execute(
                "lucas",
                Operation::Create(CreateInstance {
                    name: name.clone(),
                    source: git.url(),
                    git_passphrase: GitPassphrase::new("secret".to_owned()),
                }),
            )
            .unwrap()
    });
    let Response::Instance { instance } = created else {
        panic!("expected instance");
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert!(!instance.ssh.as_ref().unwrap().host_keys.is_empty());
    let peer_name = InstanceName::parse(format!("{}-peer", name.as_str())).unwrap();
    let peer_created = timings.run("create peer KVM world", || {
        service
            .execute(
                "lucas",
                Operation::Create(CreateInstance {
                    name: peer_name.clone(),
                    source: git.url(),
                    git_passphrase: GitPassphrase::new("secret".to_owned()),
                }),
            )
            .unwrap()
    });
    let Response::Instance {
        instance: peer_instance,
    } = peer_created
    else {
        panic!("expected peer instance");
    };
    assert_eq!(peer_instance.status, InstanceStatus::Running);
    assert_ne!(instance.guest_ip, peer_instance.guest_ip);
    assert_ne!(
        instance.ssh.as_ref().unwrap().host_keys,
        peer_instance.ssh.as_ref().unwrap().host_keys
    );

    let result = (|| {
        let Response::Instances { instances } = service.execute("lucas", Operation::List).unwrap()
        else {
            return Err("expected list response".to_owned());
        };
        assert_eq!(instances.len(), 2);
        timings.run("sync SSH inventory", || {
            wt_cli::ssh::sync(
                &instances
                    .into_iter()
                    .map(|instance| wt_cli::inventory::ContextInstance {
                        context: "local".into(),
                        instance,
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())
        })?;

        let host_alias = format!("local.{}-host", name.as_str());
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
                "test",
                "-d",
                "/workspace",
            )
            .output()
            .map_err(|error| error.to_string())
        })?;
        ensure_success("enter jsdev guest host", &output)?;
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
            "read jsdev machine ID",
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
                b"export WT_PERSISTENCE_MARKER=retained; cd /tmp; printf '%s\\n' \"$WT_PERSISTENCE_MARKER:$PWD\"\n",
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
            "-L",
            "wt-app",
            "new-window",
            "\\;",
            "list-panes",
            "-a",
            "-F",
            "'#{pane_start_command}'",
        )
        .output()
        .map_err(|error| error.to_string())?;
        ensure_success("create persistent app window", &output)?;
        let panes = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        if panes.lines().count() != 2
            || !panes
                .lines()
                .all(|command| command == "/usr/local/bin/wt-app-pane")
        {
            return Err(format!("unexpected tmux pane commands: {panes:?}"));
        }

        let branch = format!("wt-e2e-{}", std::process::id());
        let app_commands = temp.path().join("app-commands");
        fs::write(
            &app_commands,
            format!(
                "set -eu\ntest -n \"$BASH_VERSION\"\ntest \"$(id -u)\" -ne 0\ntest \"$(pwd)\" = /workspaces/jsdev\ngit config user.name wt-e2e\ngit config user.email wt@example.invalid\ngit switch -c {branch}\nprintf 'pushed\\n' > wt-e2e.txt\ngit add wt-e2e.txt\ngit commit -m wt-e2e\nprintf '#!/bin/sh\\necho secret\\n' > /tmp/wt-askpass\nchmod 0700 /tmp/wt-askpass\nDISPLAY=:0 SSH_ASKPASS=/tmp/wt-askpass SSH_ASKPASS_REQUIRE=force setsid -w git push origin HEAD:refs/heads/{branch}\nrm -f /tmp/wt-askpass\nexit\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        let input = fs::File::open(&app_commands).map_err(|error| error.to_string())?;
        let output = timings.run("push from app container", || {
            cmd!(
                "ssh",
                "-F",
                &ssh_config,
                "-i",
                &git.guest_key,
                name.as_str(),
            )
            .stdin(Stdio::from(input))
            .output()
            .map_err(|error| error.to_string())
        })?;
        ensure_success("enter and push from jsdev app container", &output)?;
        let pushed = git_output(
            cmd!(
                "git",
                "--git-dir",
                &git.repository,
                "rev-parse",
                format!("refs/heads/{branch}"),
            ),
            "verify pushed branch",
        );
        if pushed.trim().is_empty() {
            return Err("pushed branch has no commit".to_owned());
        }
        Ok(())
    })();

    let peer_removed = timings.run("remove peer KVM world", || {
        service.execute("lucas", Operation::Delete { name: peer_name })
    });
    assert!(
        peer_removed.is_ok(),
        "remove peer KVM sample world: {peer_removed:?}"
    );
    let removed = timings.run("remove KVM world", || {
        service.execute("lucas", Operation::Delete { name })
    });
    assert!(removed.is_ok(), "remove KVM sample world: {removed:?}");
    result.unwrap();
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
        let repository = root.join("jsdev-sample.git");
        run(
            cmd!("git", "clone", "--bare", SAMPLE_SOURCE, &repository),
            "create bare jsdev repository",
        );
        assert_fixture_images(&repository);
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

fn fixture_images() -> Vec<String> {
    FIXTURE_IMAGES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn assert_fixture_images(repository: &Path) {
    let compose = git_output(
        cmd!(
            "git",
            "--git-dir",
            repository,
            "show",
            "refs/heads/main:.devcontainer/compose.yaml",
        ),
        "read jsdev Compose fixture",
    );
    let actual = compose
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("image: "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        fixture_images(),
        "jsdev fixture images changed; update fixture-images.txt and rebuild the integration cache"
    );
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
