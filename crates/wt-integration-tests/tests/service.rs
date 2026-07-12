use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tempfile::TempDir;
use wt_api::{
    CreateInstance, ErrorCode, GitPassphrase, InstanceName, InstanceStatus, Operation, Response,
    SshAccess,
};
use wt_libvirt::{ProvisionSpec, WorkerError, World, WorldWorker};
use wt_server::jobs::{run_provision, JobError, JobLock, Jobs, ProcessLauncher, ProvisionLauncher};
use wt_server::service::Service;
use wt_server::store::{Store, StoredInstance};

#[derive(Clone, Debug, Default)]
struct InjectedWorker {
    fail_provision: bool,
    reject_passphrase: bool,
    provision_calls: Arc<AtomicUsize>,
    destroy_calls: Arc<AtomicUsize>,
}

#[derive(Clone, Debug)]
struct InlineLauncher;

#[derive(Clone, Debug)]
struct FailingLauncher;

impl ProvisionLauncher<InjectedWorker> for FailingLauncher {
    fn launch(
        &self,
        _store: &Store,
        _worker: &InjectedWorker,
        _stored: &StoredInstance,
        _passphrase: &GitPassphrase,
        _lock: JobLock,
    ) -> Result<(), JobError> {
        Err(JobError::Exited)
    }
}

impl ProvisionLauncher<InjectedWorker> for InlineLauncher {
    fn launch(
        &self,
        store: &Store,
        worker: &InjectedWorker,
        stored: &StoredInstance,
        passphrase: &GitPassphrase,
        _lock: JobLock,
    ) -> Result<(), JobError> {
        run_provision(store, worker, stored.clone(), passphrase)
            .map_err(|error| JobError::Io(std::io::Error::other(error)))
    }
}

impl WorldWorker for InjectedWorker {
    fn validate_git_passphrase(&self, _passphrase: &GitPassphrase) -> Result<(), WorkerError> {
        if self.reject_passphrase {
            return Err(WorkerError::new(
                "Git identity: invalid private key passphrase",
            ));
        }
        Ok(())
    }

    fn provision(
        &self,
        _spec: &ProvisionSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<World, WorkerError> {
        self.provision_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_provision {
            return Err(WorkerError::new("injected provision failure"));
        }
        Ok(world())
    }

    fn destroy(&self, _backend_id: &str) -> Result<(), WorkerError> {
        self.destroy_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn inspect(&self, _backend_id: &str) -> Result<Option<World>, WorkerError> {
        Ok(Some(world()))
    }
}

fn world() -> World {
    World {
        guest_ip: "192.0.2.2".to_owned(),
        ssh: SshAccess {
            user: "wt".to_owned(),
            host: "192.0.2.2".to_owned(),
            port: 22,
            host_keys: vec!["ssh-ed25519 AAAATEST guest".to_owned()],
        },
    }
}

fn create(name: InstanceName) -> CreateInstance {
    CreateInstance {
        name,
        source: "git@example.test:repo.git".to_owned(),
        git_passphrase: GitPassphrase::new("secret".to_owned()),
    }
}

fn service(temp: &TempDir, worker: InjectedWorker) -> Service<InjectedWorker, InlineLauncher> {
    Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        Jobs::open(temp.path().join("jobs")).unwrap(),
        InlineLauncher,
    )
}

#[test]
fn lifecycle_persists_and_is_owner_scoped() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("instances.db");
    let name = InstanceName::parse("repo-feature").unwrap();

    let mut service = Service::new(
        Store::open(&database).unwrap(),
        InjectedWorker::default(),
        Jobs::open(temp.path().join("jobs")).unwrap(),
        InlineLauncher,
    );
    let created = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap();
    let Response::Instance { instance } = created else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Provisioning);
    assert_eq!(instance.source, "git@example.test:repo.git");
    assert!(instance.ssh.is_none());

    let Response::Instance { instance } = service
        .execute("lucas", Operation::Get { name: name.clone() })
        .unwrap()
    else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert_eq!(instance.guest_ip.as_deref(), Some("192.0.2.2"));
    assert_eq!(instance.ssh.as_ref().unwrap().user, "wt");

    let conflict = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::Conflict);

    drop(service);
    let mut restarted = Service::new(
        Store::open(&database).unwrap(),
        InjectedWorker::default(),
        Jobs::open(temp.path().join("jobs")).unwrap(),
        InlineLauncher,
    );
    let Response::Instances { instances } = restarted.execute("lucas", Operation::List).unwrap()
    else {
        panic!("expected instances response");
    };
    assert_eq!(instances.len(), 1);

    let Response::Instances { instances } = restarted.execute("other", Operation::List).unwrap()
    else {
        panic!("expected instances response");
    };
    assert!(instances.is_empty());

    restarted
        .execute("lucas", Operation::Delete { name: name.clone() })
        .unwrap();
    let missing = restarted
        .execute("lucas", Operation::Get { name })
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::NotFound);
}

#[test]
fn provision_failure_is_recorded() {
    let temp = TempDir::new().unwrap();
    let mut service = service(
        &temp,
        InjectedWorker {
            fail_provision: true,
            ..InjectedWorker::default()
        },
    );

    let response = service
        .execute(
            "lucas",
            Operation::Create(create(InstanceName::parse("repo-failure").unwrap())),
        )
        .unwrap();
    let Response::Instance { instance } = response else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Provisioning);

    let Response::Instances { instances } = service.execute("lucas", Operation::List).unwrap()
    else {
        panic!("expected instances response");
    };
    assert_eq!(instances[0].status, InstanceStatus::Error);
    assert_eq!(
        instances[0].last_error.as_deref(),
        Some("injected provision failure")
    );
}

#[test]
fn invalid_git_passphrase_does_not_reserve_instance() {
    let temp = TempDir::new().unwrap();
    let provision_calls = Arc::new(AtomicUsize::new(0));
    let mut service = service(
        &temp,
        InjectedWorker {
            reject_passphrase: true,
            provision_calls: Arc::clone(&provision_calls),
            ..InjectedWorker::default()
        },
    );
    let name = InstanceName::parse("repo-passphrase").unwrap();

    let error = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidGitPassphrase);
    assert_eq!(provision_calls.load(Ordering::SeqCst), 0);

    let missing = service
        .execute("lucas", Operation::Get { name })
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::NotFound);
}

#[test]
fn create_accepts_only_ssh_sources() {
    let temp = TempDir::new().unwrap();
    let mut service = service(&temp, InjectedWorker::default());
    for source in [
        "https://github.com/example/repo.git",
        "git://example.test/repo.git",
        "/tmp/repo.git",
    ] {
        let mut request = create(InstanceName::parse("repo-invalid").unwrap());
        request.source = source.to_owned();
        let error = service
            .execute("lucas", Operation::Create(request))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest, "{source}");
    }
}

#[test]
fn detached_worker_survives_the_acknowledged_service_and_streams_sqlite_logs() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join(".local/state/wt");
    let database = state.join("instances.db");
    let jobs_dir = state.join("jobs");
    let launcher = ProcessLauncher::new(
        env!("CARGO_BIN_EXE_wt-test-server").into(),
        vec!["fake-provision".to_owned()],
    )
    .with_env("HOME", temp.path());
    let name = InstanceName::parse("detached").unwrap();
    let mut service = Service::new(
        Store::open(&database).unwrap(),
        InjectedWorker::default(),
        Jobs::open(jobs_dir.clone()).unwrap(),
        launcher.clone(),
    );

    let Response::Instance { instance } = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap()
    else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Provisioning);

    let Response::Logs {
        chunk,
        next_offset,
        status,
        ..
    } = service
        .execute(
            "lucas",
            Operation::Logs {
                name: name.clone(),
                offset: 0,
            },
        )
        .unwrap()
    else {
        panic!("expected logs response");
    };
    assert_eq!(status, InstanceStatus::Provisioning);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(chunk)
            .unwrap(),
        b"first chunk\n"
    );

    drop(service);
    std::thread::sleep(std::time::Duration::from_millis(600));
    let mut reattached = Service::new(
        Store::open(&database).unwrap(),
        InjectedWorker::default(),
        Jobs::open(jobs_dir).unwrap(),
        launcher,
    );
    let Response::Logs { chunk, status, .. } = reattached
        .execute(
            "lucas",
            Operation::Logs {
                name,
                offset: next_offset,
            },
        )
        .unwrap()
    else {
        panic!("expected logs response");
    };
    assert_eq!(status, InstanceStatus::Running);
    assert_eq!(
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(chunk)
                .unwrap()
        )
        .unwrap(),
        "second chunk\nSUCCESS: fake world is running\n"
    );
}

#[test]
fn launch_failure_removes_the_reservation() {
    let temp = TempDir::new().unwrap();
    let mut service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        InjectedWorker::default(),
        Jobs::open(temp.path().join("jobs")).unwrap(),
        FailingLauncher,
    );
    let name = InstanceName::parse("launch-failure").unwrap();

    let error = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(error.message.contains("before acknowledgement"));
    let missing = service
        .execute("lucas", Operation::Get { name })
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::NotFound);
}

#[test]
fn exited_worker_is_recovered_to_error_and_removed_explicitly() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join(".local/state/wt");
    let database = state.join("instances.db");
    let jobs_dir = state.join("jobs");
    let launcher = ProcessLauncher::new(
        env!("CARGO_BIN_EXE_wt-test-server").into(),
        vec!["fake-interrupted".to_owned()],
    )
    .with_env("HOME", temp.path());
    let destroy_calls = Arc::new(AtomicUsize::new(0));
    let worker = InjectedWorker {
        destroy_calls: Arc::clone(&destroy_calls),
        ..InjectedWorker::default()
    };
    let name = InstanceName::parse("interrupted").unwrap();
    let mut service = Service::new(
        Store::open(&database).unwrap(),
        worker.clone(),
        Jobs::open(jobs_dir.clone()).unwrap(),
        launcher.clone(),
    );
    let Response::Instance { instance } = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap()
    else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Provisioning);
    drop(service);
    std::thread::sleep(std::time::Duration::from_millis(100));

    let mut recovered = Service::new(
        Store::open(&database).unwrap(),
        worker,
        Jobs::open(jobs_dir).unwrap(),
        launcher,
    );
    let Response::Instance { instance } = recovered
        .execute("lucas", Operation::Get { name: name.clone() })
        .unwrap()
    else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Error);
    assert!(instance.last_error.unwrap().contains("interrupted"));

    let conflict = recovered
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::Conflict);
    recovered
        .execute("lucas", Operation::Delete { name: name.clone() })
        .unwrap();
    assert_eq!(destroy_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        recovered
            .execute("lucas", Operation::Get { name })
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}
use base64::Engine as _;
