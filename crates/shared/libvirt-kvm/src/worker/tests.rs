use super::{
    allocated_bytes, create_overlay_command, select_world_for_vsock_cid, shutdown_reason,
    validate_worlds_dir_details, validate_worlds_storage_dir_details, vsock_cid,
    write_creation_timing,
};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;

#[test]
fn names_known_libvirt_shutdown_reasons() {
    assert_eq!(
        shutdown_reason(virt::sys::VIR_DOMAIN_SHUTOFF_CRASHED as i32),
        Some("crashed")
    );
    assert_eq!(shutdown_reason(-1), None);
}

#[test]
fn reads_the_runtime_vsock_cid_from_libvirt_xml() {
    assert_eq!(
        vsock_cid(
            "<domain><devices><vsock model='virtio'><cid auto='yes' address='42'/></vsock></devices></domain>"
        )
        .unwrap(),
        Some(42)
    );
    assert_eq!(
        vsock_cid("<domain><devices><vsock model='virtio'/></devices></domain>").unwrap(),
        None
    );
    assert_eq!(
        vsock_cid("<domain><devices><vsock><cid address='invalid'/></vsock></devices></domain>")
            .unwrap_err()
            .to_string(),
        "libvirt domain has an invalid vsock CID"
    );
}

#[test]
fn selects_the_only_matching_wt_domain_for_a_vsock_cid() {
    let first = "wt-0123456789abcdef0123456789abcdef";
    let second = "wt-fedcba9876543210fedcba9876543210";
    let expected = uuid::Uuid::parse_str("fedcba9876543210fedcba9876543210")
        .unwrap()
        .into();

    assert_eq!(
        select_world_for_vsock_cid(
            42,
            [("other", Some(42)), (first, Some(41)), (second, Some(42)),],
        )
        .unwrap(),
        Some(expected)
    );
    assert_eq!(
        select_world_for_vsock_cid(42, [(first, None), (second, Some(41))]).unwrap(),
        None
    );
}

#[test]
fn rejects_duplicate_active_wt_vsock_cids() {
    let error = select_world_for_vsock_cid(
        42,
        [
            ("wt-0123456789abcdef0123456789abcdef", Some(42)),
            ("wt-fedcba9876543210fedcba9876543210", Some(42)),
        ],
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "multiple active WT domains use vsock CID 42"
    );
}

#[test]
fn initial_world_disk_is_a_golden_image_overlay() {
    let create = create_overlay_command(
        Path::new("/images/golden.qcow2"),
        Path::new("/world/disk"),
        48,
    );

    assert_eq!(create.get_program(), OsStr::new("qemu-img"));
    insta::assert_debug_snapshot!(create.get_args().collect::<Vec<_>>(), @r###"
    [
        "create",
        "-q",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        "/images/golden.qcow2",
        "/world/disk",
        "48G",
    ]
    "###);
}

#[test]
fn creation_timing_has_stable_precision() {
    let mut output = Vec::new();
    write_creation_timing(
        &mut output,
        "wait for guest agent",
        std::time::Duration::from_millis(1250),
    )
    .unwrap();

    insta::assert_snapshot!(String::from_utf8(output).unwrap(), @"World creation timing: wait for guest agent took 1.250s\n");
}

#[test]
fn disk_usage_reports_allocated_blocks_not_virtual_length() {
    use std::io::{Seek, Write};

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sparse.qcow2");
    let mut file = std::fs::File::create(&path).unwrap();
    file.set_len(1024 * 1024 * 1024).unwrap();
    file.rewind().unwrap();
    file.write_all(&[1; 4096]).unwrap();
    file.sync_all().unwrap();

    let usage = allocated_bytes(&path).unwrap();
    assert!(usage >= 4096);
    assert!(usage < 1024 * 1024 * 1024);
}

fn worlds_acl() -> BTreeSet<String> {
    [
        "user::rwx",
        "user:libvirt-qemu:--x",
        "group::rwx",
        "mask::rwx",
        "other::---",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn worlds_directory_keeps_the_host_kvm_boundary() {
    validate_worlds_dir_details(
        Path::new("/var/lib/libvirt/images/wt"),
        1001,
        36,
        1001,
        36,
        0o2770,
        &worlds_acl(),
    )
    .unwrap();

    let error = validate_worlds_dir_details(
        Path::new("/var/lib/libvirt/images/wt"),
        1001,
        36,
        1001,
        1001,
        0o2770,
        &worlds_acl(),
    )
    .unwrap_err();
    insta::assert_snapshot!(error.to_string(), @"worlds directory identity mismatch at /var/lib/libvirt/images/wt: expected uid=1001 gid=36 (kvm) mode=2770; actual uid=1001 gid=1001 mode=2770");
}

#[test]
fn worlds_directory_requires_qemu_traverse_access() {
    let error = validate_worlds_dir_details(
        Path::new("/var/lib/libvirt/images/wt"),
        1001,
        36,
        1001,
        36,
        0o2770,
        &BTreeSet::from([
            "user::rwx".to_owned(),
            "group::rwx".to_owned(),
            "other::---".to_owned(),
        ]),
    )
    .unwrap_err();
    insta::assert_snapshot!(error.to_string(), @"worlds directory QEMU access mismatch at /var/lib/libvirt/images/wt: expected ACL [group::rwx, mask::rwx, other::---, user::rwx, user:libvirt-qemu:--x]; actual ACL [group::rwx, other::---, user::rwx]");
}

#[test]
fn disk_node_directory_keeps_the_host_kvm_boundary() {
    validate_worlds_storage_dir_details(
        Path::new("/var/lib/libvirt/images/wt/disks"),
        1001,
        36,
        true,
        1001,
        36,
        0o2770,
    )
    .unwrap();

    let error = validate_worlds_storage_dir_details(
        Path::new("/var/lib/libvirt/images/wt/disks"),
        1001,
        36,
        true,
        1001,
        1001,
        0o2770,
    )
    .unwrap_err();
    insta::assert_snapshot!(error.to_string(), @"disk node directory identity mismatch at /var/lib/libvirt/images/wt/disks: expected non-symlink directory uid=1001 gid=36 (kvm) mode=2770; actual type=directory uid=1001 gid=1001 mode=2770");
}
