use std::fs;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;
use wt_command::cmd;

const FIXTURE_SOURCE: &str = "https://github.com/lucasavila00/small-devcontainer-fixture.git";

pub(crate) struct SshAgent {
    pub(crate) child: Child,
    pub(crate) socket: String,
}

impl SshAgent {
    pub(crate) fn start(root: &Path, identity: &Path) -> Self {
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

pub(crate) struct GitSshServer {
    pub(crate) child: Child,
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) repository: PathBuf,
    pub(crate) git_key: PathBuf,
    pub(crate) guest_key: PathBuf,
    pub(crate) guest_public_key: PathBuf,
}

impl GitSshServer {
    pub(crate) fn start(root: &Path, address: IpAddr) -> Self {
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

    pub(crate) fn url(&self) -> String {
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

pub(crate) fn generate_key(path: &Path, passphrase: &str) {
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

pub(crate) fn current_user() -> String {
    std::env::var("USER").expect("USER is set")
}

pub(crate) fn network_address(network: &str) -> IpAddr {
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

pub(crate) fn git_output(mut command: Command, action: &str) -> String {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

pub(crate) fn wait_for_line(child: &mut Child, expected: &str) -> Result<(), String> {
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

pub(crate) fn disconnect(child: &mut Child, description: &str) -> Result<(), String> {
    child
        .kill()
        .map_err(|error| format!("disconnect {description}: {error}"))?;
    child
        .wait()
        .map_err(|error| format!("wait for disconnected {description}: {error}"))?;
    Ok(())
}

pub(crate) fn run(mut command: Command, action: &str) {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn ensure_success(action: &str, output: &Output) -> Result<(), String> {
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
