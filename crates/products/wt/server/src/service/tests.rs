use super::*;
use wt_control_protocol::{ErrorCode, Operation, Outcome, Response, WorldId, WorldName};
use wt_guest::{GuestAccess, WorldInspection, WorldProvisionSpec, WorldWorker};
use wt_libvirt_kvm::WorkerError;

#[derive(Clone)]
struct UnusedWorker;

impl WorldWorker for UnusedWorker {
    fn provision(
        &self,
        _spec: WorldProvisionSpec<'_>,
        _log: &mut dyn std::io::Write,
    ) -> Result<GuestAccess, WorkerError> {
        panic!("unexpected provision")
    }

    fn destroy(&self, _world_id: WorldId) -> Result<(), WorkerError> {
        panic!("unexpected destroy")
    }

    fn inspect(&self, _world_id: WorldId) -> Result<WorldInspection, WorkerError> {
        panic!("unexpected inspect")
    }

    fn start(&self, _world_id: WorldId) -> Result<GuestAccess, WorkerError> {
        panic!("unexpected start")
    }

    fn stop(&self, _world_id: WorldId) -> Result<(), WorkerError> {
        panic!("unexpected stop")
    }

    fn disk_usage(&self, _world_id: WorldId) -> Result<u64, WorkerError> {
        panic!("unexpected disk usage")
    }
}

struct UnusedGateway;

impl AgentToolGateway for UnusedGateway {
    fn pane_observations(
        &self,
        _world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String> {
        panic!("unexpected pane observations")
    }

    fn activate_world(&self, _world_id: WorldId) -> Result<(), String> {
        panic!("unexpected activation")
    }

    fn deactivate_world(&self, _world_id: WorldId) -> Result<(), String> {
        panic!("unexpected deactivation")
    }
}

fn test_service(store: Store) -> Service<UnusedWorker, UnusedGateway> {
    Service::new(
        store,
        UnusedWorker,
        UnusedGateway,
        Operations::default(),
        u64::MAX,
    )
}

#[test]
fn api_delete_replays_after_restart_and_rejects_changed_content() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("instances.db");
    let request_id = uuid::Uuid::new_v4();
    let world_id = WorldId::new();
    let request_hash = "a".repeat(64);
    let operation = || Operation::DeleteWorld { world_id };

    let first = test_service(Store::open(&path).unwrap()).execute_api_mutation(
        "owner",
        request_id,
        Some(&request_hash),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(
        first.outcome,
        Outcome::Ok { ref response }
            if **response == (Response::WorldDeleted { world_id })
    ));
    assert!(first.expires_at_unix_ms.is_some());

    let service = test_service(Store::open(&path).unwrap());
    let replay = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&request_hash),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert_eq!(replay, first);

    let conflict = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&"b".repeat(64)),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(
        conflict.outcome,
        Outcome::Error { ref error }
            if error.code == ErrorCode::Conflict && !error.retryable
    ));
    assert!(conflict.expires_at_unix_ms.is_none());
}

#[test]
fn retryable_api_failure_does_not_consume_the_request_id() {
    let temp = tempfile::tempdir().unwrap();
    let service = test_service(Store::open(&temp.path().join("instances.db")).unwrap());
    let request_id = uuid::Uuid::new_v4();
    let world_id = WorldId::new();
    let request_hash = "a".repeat(64);
    let operation = || Operation::DeleteWorld { world_id };
    let actual_server_id = service.store.server_id().unwrap();
    let mut wrong_server_id = uuid::Uuid::new_v4();
    while wrong_server_id == actual_server_id {
        wrong_server_id = uuid::Uuid::new_v4();
    }
    let mismatch = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&request_hash),
        Some(wrong_server_id),
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(
        mismatch.outcome,
        Outcome::Error { ref error } if error.code == ErrorCode::ServerMismatch
    ));
    assert_eq!(mismatch.server_id, Some(actual_server_id));
    let active = service.operations.try_lock_world(world_id).unwrap();

    let retryable = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&request_hash),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(
        retryable.outcome,
        Outcome::Error { ref error } if error.retryable
    ));
    assert!(retryable.expires_at_unix_ms.is_none());

    drop(active);
    let completed = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&request_hash),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(completed.outcome, Outcome::Ok { .. }));
    assert!(completed.expires_at_unix_ms.is_some());
}

#[test]
fn setup_fingerprint_is_stable() {
    let request = CreateWorld {
        name: WorldName::parse("host").unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
    };

    let fingerprint = setup_fingerprint(&request).unwrap();
    assert_eq!(fingerprint.len(), 64);
    assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn retry_recovers_after_six_transient_failures() {
    let mut attempts = 0;
    let mut waits = 0;

    let result = retry(
        || {
            attempts += 1;
            (attempts > 6).then_some("running").ok_or("unresponsive")
        },
        6,
        || waits += 1,
    );

    assert_eq!(result, Ok("running"));
    assert_eq!(attempts, 7);
    assert_eq!(waits, 6);
}

#[test]
fn retry_returns_the_last_error_after_six_retries() {
    let mut attempts = 0;
    let mut waits = 0;

    let result = retry::<(), _>(
        || {
            attempts += 1;
            Err(attempts)
        },
        6,
        || waits += 1,
    );

    assert_eq!(result, Err(7));
    assert_eq!(waits, 6);
}
