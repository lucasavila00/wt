use super::*;
use wt_api::{CreateApplication, InstanceName};

#[test]
fn setup_fingerprint_does_not_store_host_user_data() {
    let secret = "token-that-must-not-be-stored";
    let request = CreateInstance {
        name: InstanceName::parse("host").unwrap(),
        vcpus: 1,
        memory_mib: 1024,
        disk_gib: 8,
        ssh_authorized_keys: vec!["ssh-ed25519 AAAATEST".into()],
        git_user_name: "Test User".into(),
        git_user_email: "test@example.invalid".into(),
        application: CreateApplication::Host {
            user_data: format!("#cloud-config\nwrite_files:\n  - content: {secret}\n"),
        },
    };

    let fingerprint = setup_fingerprint(&request).unwrap();
    assert_eq!(fingerprint.len(), 64);
    assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(!fingerprint.contains(secret));
}
