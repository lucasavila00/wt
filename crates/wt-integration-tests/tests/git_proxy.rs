use std::env;
use std::fs::{self, File};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use wt_git_proxy::{add_public_key, remove_key, ProviderConfig, ProxyConfig};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn standalone_proxy_enforces_one_authorized_keys_file_and_shared_write_policy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let upstream_repository = root.join("upstream.git");
    let seed = root.join("seed");
    git(
        root,
        &["init", "--bare", upstream_repository.to_str().unwrap()],
    );
    git(root, &["init", "-b", "main", seed.to_str().unwrap()]);
    git(&seed, &["config", "user.name", "Proxy Test"]);
    git(&seed, &["config", "user.email", "proxy@example.invalid"]);
    fs::write(seed.join("README.md"), "base\n").unwrap();
    git(&seed, &["add", "README.md"]);
    git(&seed, &["commit", "-m", "base"]);
    git(
        &seed,
        &["push", upstream_repository.to_str().unwrap(), "main:main"],
    );
    git(
        &upstream_repository,
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );

    let client_key = root.join("client");
    let upstream_key = root.join("upstream-client");
    let proxy_host_key = root.join("proxy-host");
    let upstream_host_key = root.join("upstream-host");
    for key in [
        &client_key,
        &upstream_key,
        &proxy_host_key,
        &upstream_host_key,
    ] {
        keygen(key);
    }

    let user = env::var("USER").expect("USER must be set");
    let upstream_port = unused_port();
    let proxy_port = unused_port();
    let upstream_authorized_keys = root.join("upstream-authorized-keys");
    fs::copy(
        with_extension(&upstream_key, "pub"),
        &upstream_authorized_keys,
    )
    .unwrap();
    let upstream_config = root.join("upstream-sshd.conf");
    write_sshd_config(
        &upstream_config,
        upstream_port,
        &upstream_host_key,
        &upstream_authorized_keys,
        &root.join("upstream.pid"),
    );
    let _upstream = start_sshd(&upstream_config, root.join("upstream-sshd.log"));
    let upstream_known_hosts = root.join("upstream-known-hosts");
    write_known_hosts(
        &upstream_known_hosts,
        upstream_port,
        &with_extension(&upstream_host_key, "pub"),
    );

    let config_path = root.join("proxy.toml");
    let proxy_authorized_keys = root.join("authorized_keys");
    let proxy_binary = PathBuf::from(env!("CARGO_BIN_EXE_wt-test-git-proxy"));
    let config = ProxyConfig {
        write_prefix: "tasks/".to_owned(),
        allowed_branches: vec!["main".to_owned()],
        providers: vec![ProviderConfig {
            host: "127.0.0.1".to_owned(),
            user: user.clone(),
            port: upstream_port,
            private_key_file: upstream_key,
            known_hosts_file: upstream_known_hosts,
        }],
    };
    config.save(&config_path).unwrap();
    let authorized = add_public_key(
        &config_path,
        &proxy_binary,
        "integration client",
        &fs::read_to_string(with_extension(&client_key, "pub")).unwrap(),
    )
    .unwrap();

    let proxy_config = root.join("proxy-sshd.conf");
    write_sshd_config(
        &proxy_config,
        proxy_port,
        &proxy_host_key,
        &proxy_authorized_keys,
        &root.join("proxy.pid"),
    );
    let _proxy = start_sshd(&proxy_config, root.join("proxy-sshd.log"));
    let proxy_known_hosts = root.join("proxy-known-hosts");
    write_known_hosts(
        &proxy_known_hosts,
        proxy_port,
        &with_extension(&proxy_host_key, "pub"),
    );

    let checkout = root.join("checkout");
    let url = format!(
        "ssh://{user}@127.0.0.1:{proxy_port}/127.0.0.1/{}",
        upstream_repository.display()
    );
    assert_success(&git_proxy(
        root,
        &["clone", &url, checkout.to_str().unwrap()],
        &client_key,
        &proxy_known_hosts,
    ));
    git(&checkout, &["config", "user.name", "Proxy Test"]);
    git(
        &checkout,
        &["config", "user.email", "proxy@example.invalid"],
    );

    fs::write(checkout.join("README.md"), "main update\n").unwrap();
    git(&checkout, &["commit", "-am", "main update"]);
    assert_success(&git_proxy(
        &checkout,
        &["push", "origin", "main"],
        &client_key,
        &proxy_known_hosts,
    ));

    git(&checkout, &["switch", "-c", "tasks/fix"]);
    fs::write(checkout.join("TASK.md"), "task\n").unwrap();
    git(&checkout, &["add", "TASK.md"]);
    git(&checkout, &["commit", "-m", "task"]);
    assert_success(&git_proxy(
        &checkout,
        &["push", "origin", "tasks/fix"],
        &client_key,
        &proxy_known_hosts,
    ));

    git(&checkout, &["branch", "tasks/mixed"]);
    git(&checkout, &["branch", "wrong"]);
    let rejected = git_proxy(
        &checkout,
        &["push", "origin", "tasks/mixed", "wrong"],
        &client_key,
        &proxy_known_hosts,
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("outside the write policy"));
    assert_ref(&upstream_repository, "refs/heads/tasks/mixed", false);
    assert_ref(&upstream_repository, "refs/heads/wrong", false);

    git(&checkout, &["tag", "v1"]);
    let rejected = git_proxy(
        &checkout,
        &["push", "origin", "v1"],
        &client_key,
        &proxy_known_hosts,
    );
    assert!(!rejected.status.success());
    assert_ref(&upstream_repository, "refs/tags/v1", false);

    remove_key(&config_path, &proxy_binary, &authorized.fingerprint).unwrap();
    let rejected = git_proxy(
        &checkout,
        &["fetch", "origin"],
        &client_key,
        &proxy_known_hosts,
    );
    assert!(!rejected.status.success());
}

fn git(directory: &Path, args: &[&str]) {
    assert_success(
        &Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .unwrap(),
    );
}

fn git_proxy(directory: &Path, args: &[&str], identity: &Path, known_hosts: &Path) -> Output {
    let ssh = format!(
        "ssh -i {} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile={}",
        identity.display(),
        known_hosts.display()
    );
    Command::new("git")
        .current_dir(directory)
        .env("GIT_SSH_COMMAND", ssh)
        .args(args)
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

fn keygen(path: &Path) {
    assert_success(
        &Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .output()
            .unwrap(),
    );
}

fn write_sshd_config(
    path: &Path,
    port: u16,
    host_key: &Path,
    authorized_keys: &Path,
    pid_file: &Path,
) {
    fs::write(
        path,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nPubkeyAuthentication yes\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nPermitRootLogin yes\nAllowTcpForwarding no\nX11Forwarding no\nPermitTTY no\nLogLevel VERBOSE\n",
            host_key.display(),
            pid_file.display(),
            authorized_keys.display()
        ),
    )
    .unwrap();
}

fn start_sshd(config: &Path, log: PathBuf) -> Process {
    assert_success(
        &Command::new("/usr/sbin/sshd")
            .args(["-t", "-f"])
            .arg(config)
            .output()
            .unwrap(),
    );
    let log_file = File::create(&log).unwrap();
    let mut child = Command::new("/usr/sbin/sshd")
        .args(["-D", "-e", "-f"])
        .arg(config)
        .stdout(Stdio::null())
        .stderr(log_file)
        .spawn()
        .unwrap();
    let port = fs::read_to_string(config)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Port "))
        .unwrap()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "sshd exited with {status}:\n{}",
                fs::read_to_string(&log).unwrap_or_default()
            );
        }
        assert!(Instant::now() < deadline, "sshd did not listen on {port}");
        thread::sleep(Duration::from_millis(20));
    }
    Process(child)
}

fn write_known_hosts(path: &Path, port: u16, public_key: &Path) {
    let key = fs::read_to_string(public_key).unwrap();
    let mut fields = key.split_whitespace();
    fs::write(
        path,
        format!(
            "[127.0.0.1]:{port} {} {}\n",
            fields.next().unwrap(),
            fields.next().unwrap()
        ),
    )
    .unwrap();
}

fn unused_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".");
    value.push(extension);
    value.into()
}
