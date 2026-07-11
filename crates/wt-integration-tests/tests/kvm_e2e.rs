use std::fs;
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{CreateInstance, InstanceName, InstanceStatus, Operation, Response};
use wt_libvirt::{LibvirtWorker, SiteConfig};
use wt_local::service::Service;
use wt_local::store::Store;

#[test]
fn local_service_runs_requested_devcontainer_and_strict_ssh() {
    let temp = TempDir::new().unwrap();
    let mut config = SiteConfig::load().unwrap();
    let key = temp.path().join("id_ed25519");
    run(Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&key), "create test SSH key");
    config.guest.ssh_authorized_keys_file = key.with_extension("pub");

    let bridge_ip = network_address(&config.libvirt.network);
    let fixture = GitFixture::start(temp.path(), bridge_ip);
    let worker = LibvirtWorker::new(config.worker_config().unwrap()).unwrap();
    let mut service = Service::new(Store::open(&temp.path().join("instances-v2.db")).unwrap(), worker);
    let name = InstanceName::parse(format!(
        "era15-kvm-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    )).unwrap();
    let created = service.execute("lucas", Operation::Create(CreateInstance {
        name: name.clone(), source: fixture.url(), git_ref: Some("main".to_owned()),
        identity_file: None,
    })).unwrap();
    let Response::Instance { instance } = created else { panic!("expected instance"); };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert!(!instance.ssh.as_ref().unwrap().host_keys.is_empty());

    let result = (|| {
        let Response::Instances { instances } = service.execute("lucas", Operation::List).unwrap() else {
            return Err("expected list response".to_owned());
        };
        assert_eq!(instances.len(), 1);
        std::env::set_var("HOME", temp.path());
        wt_cli::ssh::sync(&instances).map_err(|error| error.to_string())?;
        let output = Command::new("ssh")
            .args(["-i", key.to_str().unwrap(), name.as_str(),
                &format!("cd /workspace && test \"$(git rev-parse HEAD)\" = {} && test -n \"$(docker ps -q)\"", fixture.commit)])
            .output().map_err(|error| error.to_string())?;
        ensure_success("strict host-key SSH recipe check", &output)
    })();

    let removed = service.execute("lucas", Operation::Delete { name });
    assert!(removed.is_ok(), "remove KVM fixture world: {removed:?}");
    result.unwrap();
}

struct GitFixture {
    child: Child,
    address: IpAddr,
    port: u16,
    commit: String,
}

impl GitFixture {
    fn start(root: &Path, address: IpAddr) -> Self {
        let repositories = root.join("git");
        fs::create_dir(&repositories).unwrap();
        let bare = repositories.join("fixture.git");
        let sample = Path::new("/home/lucas/fluff/jsdev");
        let source = if sample.join(".git").is_dir() {
            sample.as_os_str()
        } else {
            std::ffi::OsStr::new("https://github.com/lucasavila00/jsdev-sample")
        };
        run(Command::new("git").args(["clone", "--bare"]).arg(source).arg(&bare), "clone jsdev sample fixture");
        let commit_output = Command::new("git").arg("--git-dir").arg(&bare)
            .args(["rev-parse", "refs/heads/main"]).output().unwrap();
        ensure_success("resolve jsdev main", &commit_output).unwrap();
        let commit = String::from_utf8(commit_output.stdout).unwrap().trim().to_owned();
        fs::write(bare.join("git-daemon-export-ok"), "").unwrap();

        let listener = TcpListener::bind((address, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new("git").arg("daemon").arg("--reuseaddr").arg("--export-all")
            .arg(format!("--base-path={}", repositories.display()))
            .arg(format!("--listen={address}")).arg(format!("--port={port}"))
            .arg(&repositories).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        for _ in 0..50 {
            if TcpStream::connect((address, port)).is_ok() { return Self { child, address, port, commit }; }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("git daemon did not become ready");
    }

    fn url(&self) -> String { format!("git://{}:{}/fixture.git", self.address, self.port) }
}

impl Drop for GitFixture {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

fn network_address(network: &str) -> IpAddr {
    let output = Command::new("virsh").args(["-c", wt_libvirt::LIBVIRT_URI, "net-dumpxml", network]).output().unwrap();
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

fn run(command: &mut Command, action: &str) {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
}

fn ensure_success(action: &str, output: &Output) -> Result<(), String> {
    if output.status.success() { Ok(()) } else {
        Err(format!("{action} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout).trim(), String::from_utf8_lossy(&output.stderr).trim()))
    }
}
