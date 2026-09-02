use super::*;
use std::sync::{Arc, Mutex};
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

#[derive(Clone, Default)]
struct CrashWindowWorker {
    state: Arc<Mutex<CrashWindowState>>,
}

#[derive(Default)]
struct CrashWindowState {
    attempts: usize,
    processes: usize,
    window_id: Option<wt_world::WindowId>,
}

impl WorldWorker for CrashWindowWorker {
    fn provision(
        &self,
        _: WorldProvisionSpec<'_>,
        _: &mut dyn std::io::Write,
    ) -> Result<GuestAccess, WorkerError> {
        unreachable!()
    }
    fn destroy(&self, _: WorldId) -> Result<(), WorkerError> {
        unreachable!()
    }
    fn inspect(&self, _: WorldId) -> Result<WorldInspection, WorkerError> {
        unreachable!()
    }
    fn start(&self, _: WorldId) -> Result<GuestAccess, WorkerError> {
        unreachable!()
    }
    fn stop(&self, _: WorldId) -> Result<(), WorkerError> {
        unreachable!()
    }
    fn disk_usage(&self, _: WorldId) -> Result<u64, WorkerError> {
        unreachable!()
    }

    fn start_window(
        &self,
        _: WorldId,
        launch: &wt_guest::WindowLaunch,
    ) -> Result<wt_guest::WindowStarted, WorkerError> {
        let mut state = self.state.lock().unwrap();
        state.attempts += 1;
        match state.window_id {
            Some(existing) => assert_eq!(existing, launch.window_id),
            None => {
                state.window_id = Some(launch.window_id);
                state.processes += 1;
            }
        }
        if state.attempts == 1 {
            return Err(WorkerError::new("connection lost after guest launch"));
        }
        Ok(wt_guest::WindowStarted {
            tmux_window_id: "@7".into(),
        })
    }
}

#[test]
fn start_window_retry_recovers_the_reserved_identity_without_a_second_process() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("instances.db")).unwrap();
    let world_id = WorldId::new();
    store
        .insert(&wt_workload_registry::NewWorld {
            world_id,
            owner: "owner".into(),
            name: WorldName::parse("host").unwrap(),
            status: wt_control_protocol::WorldStatus::Running,
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            setup_fingerprint: "test".into(),
        })
        .unwrap();
    let worker = CrashWindowWorker::default();
    let service = Service::new(
        store,
        worker.clone(),
        UnusedGateway,
        Operations::default(),
        u64::MAX,
    );
    let request_id = uuid::Uuid::new_v4();
    let operation = || {
        Operation::StartWindow(wt_control_protocol::StartWindow {
            world_id,
            argv: vec!["cat".into()],
            cwd: "/home/wt".into(),
            window_id: None,
            control_token: None,
        })
    };
    let first = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&"a".repeat(64)),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(matches!(first.outcome, Outcome::Error { ref error } if error.retryable));
    let second = service.execute_api_mutation(
        "owner",
        request_id,
        Some(&"a".repeat(64)),
        None,
        operation(),
        &mut std::io::sink(),
    );
    assert!(
        matches!(second.outcome, Outcome::Ok { ref response } if matches!(**response, Response::WindowStarted { .. }))
    );
    let state = worker.state.lock().unwrap();
    assert_eq!(state.attempts, 2);
    assert_eq!(state.processes, 1);
}

#[test]
fn start_window_rejects_an_active_world_operation() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("instances.db")).unwrap();
    let world_id = WorldId::new();
    store
        .insert(&wt_workload_registry::NewWorld {
            world_id,
            owner: "owner".into(),
            name: WorldName::parse("host").unwrap(),
            status: wt_control_protocol::WorldStatus::Running,
            vcpus: 1,
            memory_mib: 1024,
            disk_gib: 8,
            setup_fingerprint: "test".into(),
        })
        .unwrap();
    let service = test_service(store);
    let _active = service.operations.try_lock_world(world_id).unwrap();

    let error = service
        .execute(
            "owner",
            Operation::StartWindow(wt_control_protocol::StartWindow {
                world_id,
                argv: vec!["cat".into()],
                cwd: "/home/wt".into(),
                window_id: Some(wt_world::WindowId::new()),
                control_token: Some("token".into()),
            }),
        )
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(error.retryable);
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
fn api_delete_window_treats_an_absent_window_as_the_desired_state() {
    let temp = tempfile::tempdir().unwrap();
    let service = test_service(Store::open(&temp.path().join("instances.db")).unwrap());
    let window_id = wt_world::WindowId::new();
    let response = service.execute_api_mutation(
        "owner",
        uuid::Uuid::new_v4(),
        Some(&"a".repeat(64)),
        None,
        Operation::DeleteWindow {
            window_id,
            control_token: "already-gone".into(),
        },
        &mut std::io::sink(),
    );
    assert!(matches!(
        response.outcome,
        Outcome::Ok { ref response }
            if **response == Response::WindowDeleted { window_id }
    ));
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
