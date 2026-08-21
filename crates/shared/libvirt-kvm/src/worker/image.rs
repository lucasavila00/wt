use std::path::Path;
use wt_command::cmd;
use wt_libvirt_kvm::WorkerError;

use super::context;

pub(super) fn read_virtual_size(path: &Path) -> Result<u64, WorkerError> {
    let output = cmd!("qemu-img", "info", "--output=json", path)
        .output()
        .map_err(|error| context("read guest image size", error))?;
    if !output.status.success() {
        return Err(WorkerError::new(format!(
            "read guest image size: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_virtual_size(&output.stdout)
}

pub(super) fn parse_virtual_size(output: &[u8]) -> Result<u64, WorkerError> {
    serde_json::from_slice::<serde_json::Value>(output)
        .map_err(|error| context("decode guest image information", error))?
        .get("virtual-size")
        .and_then(serde_json::Value::as_u64)
        .filter(|size| *size > 0)
        .ok_or_else(|| WorkerError::new("guest image has no positive virtual size"))
}

pub(super) fn validate_disk_size(
    disk_gib: u64,
    image_virtual_size: u64,
) -> Result<(), WorkerError> {
    const GIB: u64 = 1024 * 1024 * 1024;
    let requested = disk_gib.saturating_mul(GIB);
    if requested >= image_virtual_size {
        return Ok(());
    }
    let minimum_gib = image_virtual_size.div_ceil(GIB);
    Err(WorkerError::new(format!(
        "machine disk is {disk_gib} GiB but the guest image requires at least {minimum_gib} GiB"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_qemu_image_virtual_size() {
        assert_eq!(
            parse_virtual_size(br#"{"virtual-size": 34359738368}"#).unwrap(),
            32 * 1024 * 1024 * 1024
        );
        insta::assert_snapshot!(
            parse_virtual_size(br#"{"format": "qcow2"}"#).unwrap_err(),
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
}
