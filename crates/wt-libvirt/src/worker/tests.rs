use super::{parse_image_virtual_size, shutdown_reason, validate_disk_size};

#[test]
fn names_known_libvirt_shutdown_reasons() {
    assert_eq!(
        shutdown_reason(virt::sys::VIR_DOMAIN_SHUTOFF_CRASHED as i32),
        Some("crashed")
    );
    assert_eq!(shutdown_reason(-1), None);
}

#[test]
fn parses_qemu_image_virtual_size() {
    assert_eq!(
        parse_image_virtual_size(br#"{"virtual-size": 34359738368}"#).unwrap(),
        32 * 1024 * 1024 * 1024
    );
    insta::assert_snapshot!(
        parse_image_virtual_size(br#"{"format": "qcow2"}"#).unwrap_err(),
        @"guest image has no positive virtual size"
    );
}

#[test]
fn rejects_disks_smaller_than_the_guest_image() {
    let gib = 1024 * 1024 * 1024;
    validate_disk_size(32, 32 * gib).unwrap();
    insta::assert_snapshot!(
        validate_disk_size(16, 32 * gib).unwrap_err(),
        @"machine disk is 16 GiB but the guest image requires at least 32 GiB"
    );
    insta::assert_snapshot!(
        validate_disk_size(32, 32 * gib + 1).unwrap_err(),
        @"machine disk is 32 GiB but the guest image requires at least 33 GiB"
    );
}
