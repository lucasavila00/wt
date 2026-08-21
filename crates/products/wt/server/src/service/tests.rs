use super::*;
use wt_control_protocol::InstanceName;

#[test]
fn setup_fingerprint_is_stable() {
    let request = CreateInstance {
        name: InstanceName::parse("host").unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        ssh_authorized_keys: vec!["ssh-ed25519 AAAATEST".into()],
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
    };

    let fingerprint = setup_fingerprint(&request).unwrap();
    assert_eq!(fingerprint.len(), 64);
    assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
}
