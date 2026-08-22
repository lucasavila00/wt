use super::{allocated_bytes, create_overlay_command, shutdown_reason};
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
