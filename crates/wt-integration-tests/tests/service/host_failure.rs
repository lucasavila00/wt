use super::*;

#[test]
fn host_create_rejects_ssh_identity_overrides_before_provisioning() {
    let temp = TempDir::new().unwrap();
    let worker = Worker::default();
    let error = service(&temp, worker.clone())
        .execute(
            "tester",
            Operation::Create(create_host(
                "ubuntu",
                "#cloud-config\nssh_keys:\n  ed25519_private: forbidden\n",
            )),
        )
        .unwrap_err();

    assert_eq!(error.code, wt_api::ErrorCode::InvalidRequest);
    insta::assert_snapshot!(error.message, @"cloud-init user-data cannot set top-level ssh_keys because WT owns the guest SSH identity");
    assert_eq!(worker.provisions.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_host_create_is_retained_until_explicit_delete() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        provision_error: true,
        ..Worker::default()
    };
    let service = service(&temp, worker.clone());
    let name = InstanceName::parse("ubuntu").unwrap();

    let error = service
        .execute(
            "tester",
            Operation::Create(create_host(name.as_str(), "#cloud-config\n")),
        )
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Backend);
    insta::assert_snapshot!(error.message, @"provision failed; host world 'ubuntu' was retained in error state; run `wt rm ubuntu` to delete it");
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 0);

    let stored = Store::open(&temp.path().join("instances.db"))
        .unwrap()
        .get("tester", &name)
        .unwrap();
    assert_eq!(stored.instance.status, InstanceStatus::Error);
    assert_eq!(
        stored.instance.last_error.as_deref(),
        Some("provision failed")
    );

    let retry = service
        .execute(
            "tester",
            Operation::Create(create_host(name.as_str(), "#cloud-config\n")),
        )
        .unwrap_err();
    assert_eq!(retry.code, wt_api::ErrorCode::Conflict);

    service
        .execute("tester", Operation::Delete { name: name.clone() })
        .unwrap();
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Store::open(&temp.path().join("instances.db"))
            .unwrap()
            .get("tester", &name),
        Err(wt_server::store::StoreError::NotFound)
    ));
}

#[test]
fn failed_host_setup_is_reconciled_and_retained() {
    let temp = TempDir::new().unwrap();
    let name = InstanceName::parse("ubuntu").unwrap();
    service(&temp, Worker::default())
        .execute(
            "tester",
            Operation::Create(create_host(name.as_str(), "#cloud-config\n")),
        )
        .unwrap();

    let worker = Worker {
        host_setup_error: true,
        ..Worker::default()
    };
    let Response::Instances { instances } = service(&temp, worker.clone())
        .execute("tester", Operation::List)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(instances[0].status, InstanceStatus::Error);
    insta::assert_snapshot!(instances[0].last_error.as_deref().unwrap(), @"guest reconciliation: host cloud-init failed: cloud-init final stage failed with exit status 1");
    assert_eq!(worker.destroys.load(Ordering::SeqCst), 0);

    service(&temp, worker)
        .execute("tester", Operation::Delete { name })
        .unwrap();
}
