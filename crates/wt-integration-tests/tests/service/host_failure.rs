use super::*;

#[test]
fn failed_host_create_is_retained_until_explicit_delete() {
    let temp = TempDir::new().unwrap();
    let worker = Worker {
        provision_error: true,
        ..Worker::default()
    };
    let service = service(&temp, worker.clone());
    let name = InstanceName::parse("ubuntu").unwrap();

    let mut progress = Vec::new();
    let error = service
        .execute_with_progress(
            "tester",
            Operation::Create(create_host(name.as_str(), "#cloud-config\n")),
            &mut progress,
        )
        .unwrap_err();
    assert_eq!(error.code, wt_api::ErrorCode::Backend);
    insta::assert_snapshot!(error.message, @"provision failed; host world 'ubuntu' was retained in error state; run `wt rm ubuntu` to delete it");
    insta::assert_snapshot!(String::from_utf8(progress).unwrap(), @r###"
    cloud-init stdout
    cloud-init stderr
    "###);
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
