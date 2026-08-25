use super::*;
use wt_control_protocol::WorldName;

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
