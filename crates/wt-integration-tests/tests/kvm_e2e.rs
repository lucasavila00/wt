use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{CreateInstance, InstanceName, InstanceStatus, Operation, Response};
use wt_libvirt::{LibvirtWorker, SiteConfig};
use wt_local::service::Service;
use wt_local::store::Store;

const SAMPLE_SOURCE: &str = "https://github.com/lucasavila00/jsdev-sample.git";

#[test]
fn local_service_runs_requested_devcontainer_and_strict_ssh() {
    let temp = TempDir::new().unwrap();
    let mut config = SiteConfig::load().unwrap();
    let key = temp.path().join("id_ed25519");
    run(Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&key), "create test SSH key");
    config.guest.ssh_authorized_keys_file = key.with_extension("pub");
    let commit = sample_main_commit();

    let worker = LibvirtWorker::new(config.worker_config().unwrap()).unwrap();
    let mut service = Service::new(Store::open(&temp.path().join("instances-v2.db")).unwrap(), worker);
    let name = InstanceName::parse(format!(
        "era15-kvm-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    )).unwrap();
    let created = service.execute("lucas", Operation::Create(CreateInstance {
        name: name.clone(), source: SAMPLE_SOURCE.to_owned(), git_ref: Some(commit.clone()),
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
        let recipe_check = format!(
            "cd /workspace && test \"$(git rev-parse HEAD)\" = {commit} && test -n \"$(docker ps -q)\""
        );
        let output = Command::new("ssh")
            .args(["-i", key.to_str().unwrap(), name.as_str(), &recipe_check])
            .output().map_err(|error| error.to_string())?;
        ensure_success("strict host-key SSH recipe check", &output)
    })();

    let removed = service.execute("lucas", Operation::Delete { name });
    assert!(removed.is_ok(), "remove KVM sample world: {removed:?}");
    result.unwrap();
}

fn sample_main_commit() -> String {
    let output = Command::new("git")
        .args(["ls-remote", SAMPLE_SOURCE, "refs/heads/main"])
        .output().unwrap();
    ensure_success("resolve jsdev main", &output).unwrap();
    String::from_utf8(output.stdout).unwrap()
        .split_whitespace().next().expect("jsdev main commit").to_owned()
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
