use tempfile::TempDir;
use wt_api::{
    CreateInstance, ErrorCode, InstanceName, InstanceStatus, Operation, Response, SshAccess,
};
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
        ssh: SshAccess {
            user: "wt".to_owned(),
            host: "192.0.2.2".to_owned(),
            port: 22,
            host_keys: vec!["ssh-ed25519 AAAATEST guest".to_owned()],
        },
    }
}

fn create(name: InstanceName) -> CreateInstance {
    let identity_file = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
        .join(".ssh/id_ed25519")
        .to_string_lossy()
        .into_owned();
    CreateInstance {
        name,
        source: "git@example.test:repo.git".to_owned(),
        git_ref: Some("feature".to_owned()),
        identity_file,
    }
}

#[test]
fn lifecycle_persists_and_is_owner_scoped() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("instances-v2.db");
    let name = InstanceName::parse("repo-feature").unwrap();

    let mut service = Service::new(Store::open(&database).unwrap(), InjectedWorker::default());
    let created = service
        .execute("lucas", Operation::Create(create(name.clone())))
        .unwrap();
    let Response::Instance { instance } = created else {
        panic!("expected instance response");
    };
    assert_eq!(instance.status, InstanceStatus::Running);
    assert_eq!(instance.guest_ip.as_deref(), Some("192.0.2.2"));
    assert_eq!(instance.source, "git@example.test:repo.git");
    assert_eq!(instance.git_ref.as_deref(), Some("feature"));
    assert_eq!(instance.ssh.as_ref().unwrap().user, "wt");

    let conflict = service
        .execute("lucas", Operation::Create(create(name.clone())))
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
        Store::open(&temp.path().join("instances-v2.db")).unwrap(),
        InjectedWorker {
            fail_provision: true,
        },
    );

    let error = service
        .execute(
            "lucas",
            Operation::Create(create(InstanceName::parse("repo-failure").unwrap())),
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

#[test]
fn create_accepts_only_ssh_sources() {
    let temp = TempDir::new().unwrap();
    let mut service = Service::new(
        Store::open(&temp.path().join("instances-v2.db")).unwrap(),
        InjectedWorker::default(),
    );
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
