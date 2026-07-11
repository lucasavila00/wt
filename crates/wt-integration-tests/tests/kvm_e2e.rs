use std::fs;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{CreateInstance, InstanceName, InstanceStatus, Operation, Response};
use wt_libvirt::{LibvirtWorker, SiteConfig};
use wt_local::service::Service;
use wt_local::store::Store;

const SAMPLE_FALLBACK: &str = "https://github.com/lucasavila00/jsdev-sample.git";

#[test]
fn local_service_runs_and_pushes_from_jsdev_devcontainer() {
    let temp = TempDir::new().unwrap();
    let mut config = SiteConfig::load().unwrap();
    let bridge_ip = network_address(&config.libvirt.network);
    let git = GitSshServer::start(temp.path(), bridge_ip);
    config.guest.ssh_authorized_keys_file = git.client_public_key.clone();
    std::env::set_var("HOME", temp.path());

    let worker = LibvirtWorker::new(config.worker_config().unwrap()).unwrap();
    let mut service = Service::new(
        Store::open(&temp.path().join("instances-v2.db")).unwrap(),
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
    let created = service
        .execute(
            "lucas",
            Operation::Create(CreateInstance {
                name: name.clone(),
                source: git.url(),
                git_ref: Some(git.main_commit.clone()),
                identity_file: git.client_key.to_string_lossy().into_owned(),
            }),
        )
        .unwrap();
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
        wt_cli::ssh::sync(&instances).map_err(|error| error.to_string())?;
        let branch = format!("wt-e2e-{}", std::process::id());
        let command = format!(
            "cd /workspace && /usr/local/bin/devcontainer exec --workspace-folder /workspace /bin/sh -c 'git config user.name wt-e2e && git config user.email wt@example.invalid && git switch -c {branch} && printf pushed\\n > wt-e2e.txt && git add wt-e2e.txt && git commit -m wt-e2e && git push origin HEAD:refs/heads/{branch}'"
        );
        let output = Command::new("ssh")
            .args([
                "-i",
                git.client_key.to_str().unwrap(),
                name.as_str(),
                &command,
            ])
            .output()
            .map_err(|error| error.to_string())?;
        ensure_success("push from jsdev devcontainer", &output)?;
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

    let removed = service.execute("lucas", Operation::Delete { name });
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
        let local_sample = workspace_root()
            .parent()
            .expect("workspace root has a parent")
            .join("jsdev");
        let source = if local_sample.join(".git").is_dir() {
            local_sample.into_os_string()
        } else {
            SAMPLE_FALLBACK.into()
        };
        run(
            Command::new("git")
                .args(["clone", "--bare"])
                .arg(source)
                .arg(&repository),
            "create bare jsdev repository",
        );
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
