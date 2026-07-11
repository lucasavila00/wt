use tempfile::TempDir;
use wt_api::{CreateInstance, ErrorCode, InstanceName, InstanceStatus, Operation, Response};
use wt_libvirt::{ProvisionSpec, WorkerError, World, WorldWorker};
use wt_local::service::Service;
use wt_local::store::Store;

#[derive(Clone, Debug, Default)]
struct InjectedWorker {
    fail_provision: bool,
}

impl WorldWorker for InjectedWorker {
    fn provision(&self, _spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        if self.fail_provision {
            return Err(WorkerError::new("injected provision failure"));
        }
        Ok(world())
    }

    fn destroy(&self, _backend_id: &str) -> Result<(), WorkerError> {
        Ok(())
    }

    fn inspect(&self, _backend_id: &str) -> Result<Option<World>, WorkerError> {
        Ok(Some(world()))
    }
}

fn world() -> World {
    World {
        guest_ip: "192.0.2.2".to_owned(),
    }
}

#[test]
fn lifecycle_persists_and_is_owner_scoped() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("instances.db");
    let name = InstanceName::parse("repo-feature").unwrap();

    let mut service = Service::new(Store::open(&database).unwrap(), InjectedWorker::default());
    let created = service
        .execute(
            "lucas",
            Operation::Create(CreateInstance {
                source: "git@example.com:team/repo.git".to_owned(),
                name: name.clone(),
                git_ref: Some("feature".to_owned()),
            }),
        )
        .unwrap();
    let Response::Instance { instance } = created else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert_eq!(instance.guest_ip.as_deref(), Some("192.0.2.2"));

    let conflict = service
        .execute(
            "lucas",
            Operation::Create(CreateInstance {
                source: "anything".to_owned(),
                name: name.clone(),
                git_ref: None,
            }),
        )
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::Conflict);

    drop(service);
    let mut restarted = Service::new(Store::open(&database).unwrap(), InjectedWorker::default());
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
    let mut service = Service::new(
        Store::open(&temp.path().join("instances.db")).unwrap(),
        InjectedWorker {
            fail_provision: true,
        },
    );

    let error = service
        .execute(
            "lucas",
            Operation::Create(CreateInstance {
                source: "source".to_owned(),
                name: InstanceName::parse("repo-failure").unwrap(),
                git_ref: None,
            }),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Backend);

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
