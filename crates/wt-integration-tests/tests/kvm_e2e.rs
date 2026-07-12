use std::fs;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{CreateInstance, InstanceName, InstanceStatus, Operation, Response};
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
        run(
            Command::new(env!("CARGO"))
                .current_dir(&workspace)
                .args(["build", "-p", "wt-guest"]),
            "build guest helpers",
        )
    });
    let mut config = ServerConfig::load().unwrap();
    assert_eq!(config.registry_cache.preload_images, fixture_images());
    config.install.binary_dir = workspace.join("target/debug");
    let bridge_ip = network_address(&config.libvirt.network);
    let git = timings.run("prepare SSH Git fixture", || {
        GitSshServer::start(temp.path(), bridge_ip)
    });
    config.guest.ssh_authorized_keys_file = git.client_public_key.clone();
    config.git.identity_file = git.client_key.clone();
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
                    git_ref: Some(git.main_commit.clone()),
                }),
            )
            .unwrap()
    });
    let Response::Instance { instance } = created else {
        panic!("expected instance");
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert!(!instance.ssh.as_ref().unwrap().host_keys.is_empty());

    let result = (|| {
        let Response::Instances { instances } = service.execute("lucas", Operation::List).unwrap()
        else {
            return Err("expected list response".to_owned());
        };
        assert_eq!(instances.len(), 1);
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
        let ssh_config = temp.path().join(".ssh/config");
        let output = timings.run("verify guest SSH", || {
            Command::new("ssh")
                .arg("-F")
                .arg(&ssh_config)
                .args(["-i", git.client_key.to_str().unwrap(), &host_alias])
                .args(["test", "-d", "/workspace"])
                .output()
                .map_err(|error| error.to_string())
        })?;
        ensure_success("enter jsdev guest host", &output)?;

        let branch = format!("wt-e2e-{}", std::process::id());
        let app_commands = temp.path().join("app-commands");
        fs::write(
            &app_commands,
            format!(
                "set -eu\ntest -n \"$BASH_VERSION\"\ntest \"$(id -u)\" -ne 0\ntest \"$(pwd)\" = /workspaces/jsdev\ngit config user.name wt-e2e\ngit config user.email wt@example.invalid\ngit switch -c {branch}\nprintf 'pushed\\n' > wt-e2e.txt\ngit add wt-e2e.txt\ngit commit -m wt-e2e\ngit push origin HEAD:refs/heads/{branch}\nexit\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        let input = fs::File::open(&app_commands).map_err(|error| error.to_string())?;
        let output = timings.run("push from app container", || {
            Command::new("ssh")
                .arg("-F")
                .arg(&ssh_config)
                .args(["-i", git.client_key.to_str().unwrap(), name.as_str()])
                .stdin(Stdio::from(input))
                .output()
                .map_err(|error| error.to_string())
        })?;
        ensure_success("enter and push from jsdev app container", &output)?;
        let pushed = git_output(
            Command::new("git")
                .arg("--git-dir")
                .arg(&git.repository)
                .args(["rev-parse", &format!("refs/heads/{branch}")]),
            "verify pushed branch",
        );
        if pushed.trim().is_empty() {
            return Err("pushed branch has no commit".to_owned());
        }
        Ok(())
    })();

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
    client_key: PathBuf,
    client_public_key: PathBuf,
    main_commit: String,
}

impl GitSshServer {
    fn start(root: &Path, address: IpAddr) -> Self {
        let repository = root.join("jsdev-sample.git");
        run(
            Command::new("git")
                .args(["clone", "--bare", SAMPLE_SOURCE])
                .arg(&repository),
            "create bare jsdev repository",
        );
        assert_fixture_images(&repository);
        let main_commit = git_output(
            Command::new("git")
                .arg("--git-dir")
                .arg(&repository)
                .args(["rev-parse", "refs/heads/main"]),
            "resolve jsdev main",
        )
        .trim()
        .to_owned();

        let client_key = root.join("git-client");
        let host_key = root.join("ssh-host");
        generate_key(&client_key);
        generate_key(&host_key);
        let client_public_key = client_key.with_extension("pub");
        let authorized_keys = root.join("authorized_keys");
        fs::copy(&client_public_key, &authorized_keys).unwrap();

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
        let mut child = Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
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
                    client_key,
                    client_public_key,
                    main_commit,
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
        Command::new("git")
            .arg("--git-dir")
            .arg(repository)
            .args(["show", "refs/heads/main:.devcontainer/compose.yaml"]),
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

fn generate_key(path: &Path) {
    run(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path),
        "generate test SSH key",
    );
}

fn current_user() -> String {
    std::env::var("USER").expect("USER is set")
}

fn network_address(network: &str) -> IpAddr {
    let output = Command::new("virsh")
        .args(["-c", wt_libvirt::LIBVIRT_URI, "net-dumpxml", network])
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

fn git_output(command: &mut Command, action: &str) -> String {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn run(command: &mut Command, action: &str) {
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
