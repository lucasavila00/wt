use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tempfile::TempDir;
use wt_api::{CreateInstance, InstanceName, InstanceStatus, Operation, Response, SshAccess};
use wt_provider::{ProvisionSpec, WorkerError, World, WorldWorker};
use wt_server::jobs::Jobs;
use wt_server::service::Service;
use wt_server::store::Store;

#[derive(Clone, Default)]
struct Worker {
    provisions: Arc<AtomicUsize>,
    destroys: Arc<AtomicUsize>,
    complete: bool,
}

impl WorldWorker for Worker {
    fn provision(
        &self,
        _spec: &ProvisionSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<World, WorkerError> {
        self.provisions.fetch_add(1, Ordering::SeqCst);
        Ok(world(false))
    }
    fn destroy(&self, _backend_id: &str) -> Result<(), WorkerError> {
        self.destroys.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn inspect(&self, _backend_id: &str) -> Result<Option<World>, WorkerError> {
        Ok(Some(world(self.complete)))
    }
}

fn world(complete: bool) -> World {
    World {
        guest_ip: "192.0.2.2".into(),
        ssh: SshAccess {
            user: "wt".into(),
            host: "192.0.2.2".into(),
            port: 22,
            host_keys: vec!["ssh-ed25519 AAAATEST guest".into()],
        },
        app_ssh: complete.then(|| wt_api::AppSshAccess {
            user: "vscode".into(),
            port: 2222,
            host_keys: vec!["ssh-ed25519 AAAAAPP app".into()],
        }),
    }
}

fn create(name: &str) -> CreateInstance {
    CreateInstance {
        name: InstanceName::parse(name).unwrap(),
        source: "git@example.test:repo.git".into(),
        git_branch: None,
        git_ref: None,
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
    }
}

fn service(temp: &TempDir, worker: Worker) -> Service<Worker> {
    Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        worker,
        Jobs::open(temp.path().join("jobs")).unwrap(),
    )
}

#[test]
fn create_returns_setup_ready_world_synchronously() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let calls = worker.provisions.clone();
    let Response::Instance { instance } = service(&temp, worker)
        .execute("tester", Operation::Create(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instance.status, InstanceStatus::Setup);
    assert!(instance.ssh.is_some());
    assert!(instance.app_ssh.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn list_reconciles_completed_setup_to_running() {
    let temp = TempDir::new().unwrap();
    service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    let Response::Instances { instances } = service(
        &temp,
        Worker {
            complete: true,
            ..Worker::default()
        },
    )
    .execute("tester", Operation::List)
    .unwrap() else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Running);
    assert!(instances[0].app_ssh.is_some());
}

#[test]
fn delete_removes_setup_world() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let destroys = worker.destroys.clone();
    service(&temp, worker.clone())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap();
    service(&temp, worker)
        .execute(
            "tester",
            Operation::Delete {
                name: InstanceName::parse("sample").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(destroys.load(Ordering::SeqCst), 1);
}

#[test]
fn repeated_create_resumes_only_identical_setup() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let provisions = worker.provisions.clone();
    let mut first = create("sample");
    first.git_branch = Some("feature".into());
    let Response::Instance { instance: original } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(first))
        .unwrap()
    else {
        panic!()
    };
    let mut same = create("sample");
    same.git_branch = Some("feature".into());
    let Response::Instance { instance: resumed } = service(&temp, worker.clone())
        .execute("tester", Operation::Create(same))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(resumed.id, original.id);
    assert_eq!(provisions.load(Ordering::SeqCst), 1);

    let mut different = create("sample");
    different.git_branch = Some("other".into());
    let error = service(&temp, worker)
        .execute("tester", Operation::Create(different))
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Conflict);
}
