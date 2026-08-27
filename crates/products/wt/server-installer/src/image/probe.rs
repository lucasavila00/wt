use super::*;
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};
use wt_libvirt_kvm::{
    GuestTransport, LibvirtProvider, MachineInspection, MachineProvider, MachineSpec, RunRequest,
};
use wt_world::WorldId;

const MARKER_NAME: &str = ".wt-image-publication-probe";

pub(super) fn verify_publication(
    input: &InstallInput,
    server: &ServerConfig,
    image: &Path,
) -> Result<()> {
    println!("Probing guest identity through virtiofs...");
    wt_server::validate_process_identity().map_err(anyhow::Error::msg)?;
    wt_server::validate_shared_roots(server.codex_paths()).map_err(anyhow::Error::msg)?;
    let world_id = WorldId::from(uuid::Uuid::nil());
    let marker = Path::new(server.codex_paths().sessions)
        .join(world_id.to_string())
        .join(MARKER_NAME);
    for (label, path) in [("marker", marker.as_path())] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => bail!(
                "stale shared identity probe {label} exists: {}",
                path.display()
            ),
            Err(error) => {
                return Err(error).with_context(|| format!("inspect shared identity probe {label}"))
            }
        }
    }
    let nonce = uuid::Uuid::new_v4().to_string();
    let guest_uid = wt_guest::GUEST_UID.to_string();
    let guest_gid = wt_guest::GUEST_GID.to_string();
    let expected_contents = format!("{nonce}\n").into_bytes();
    let mut machine_config = server.machine_config();
    machine_config.image = image.to_path_buf();
    let provider = LibvirtProvider::new(machine_config).map_err(anyhow::Error::msg)?;
    match provider.inspect(world_id) {
        Ok(MachineInspection::Missing) => {}
        Ok(other) => bail!("stale shared identity probe machine exists: {other:?}"),
        Err(error) => bail!("inspect shared identity probe machine: {error}"),
    }
    let spec = MachineSpec {
        world_id,
        memory_mib: input.image.build_memory_mib,
        vcpus: input.image.build_vcpus,
        disk_gib: input.image.build_disk_gib,
    };
    let result = (|| {
        let machine = provider
            .create(&spec, &mut std::io::sink())
            .map_err(anyhow::Error::msg)?;
        let deadline = Instant::now() + Duration::from_secs(server.guest.boot_timeout_seconds);
        run_guest(
            machine.transport.as_ref(),
            wt_guest::MOUNT_CODEX_HELPER,
            &[],
            deadline,
            "mount Codex shared files for shared identity probe",
        )?;
        run_guest(
            machine.transport.as_ref(),
            "/usr/sbin/runuser",
            &[
                "--user",
                wt_guest::GUEST_USER,
                "--",
                "/bin/sh",
                "-c",
                "set -euC; test \"$(id -u)\" = \"$2\"; test \"$(id -g)\" = \"$3\"; test \"$(findmnt --noheadings --output SOURCE,FSTYPE --mountpoint /home/wt/.codex/sessions | awk 'NR == 1 { print $1 \" \" $2 }')\" = 'wt-codex-integration-sessions virtiofs'; umask 077; printf '%s\\n' \"$1\" > /home/wt/.codex/sessions/.wt-image-publication-probe; test \"$(stat -c '%u:%g:%a' /home/wt/.codex/sessions/.wt-image-publication-probe)\" = \"$2:$3:600\"",
                "wt-image-publication-probe",
                &nonce,
                &guest_uid,
                &guest_gid,
            ],
            deadline,
            "write private shared identity probe as guest wt",
        )?;
        validate_marker(&marker, &expected_contents)
    })();
    let cleanup = cleanup(&provider, world_id, &marker, &expected_contents);
    match (result, cleanup) {
        (Ok(()), Ok(())) => {
            println!("Validated host/guest virtiofs identity with a private 0600 file.");
            Ok(())
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(cleanup)) => Err(cleanup.context("clean up shared identity probe machine")),
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "shared identity probe cleanup also failed: {cleanup:#}"
        ))),
    }
}

fn cleanup(
    provider: &LibvirtProvider,
    world_id: WorldId,
    marker: &Path,
    expected_contents: &[u8],
) -> Result<()> {
    let mut failures = Vec::new();
    match fs::symlink_metadata(marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            match fs::read(marker) {
                Ok(contents) if contents == expected_contents => {
                    if let Err(error) = fs::remove_file(marker) {
                        failures.push(format!("remove shared identity probe marker: {error}"));
                    }
                }
                Ok(_) => failures.push(
                    "shared identity probe marker content changed; refusing to remove it"
                        .to_owned(),
                ),
                Err(error) => failures.push(format!("read shared identity probe marker: {error}")),
            }
        }
        Ok(_) => failures
            .push("shared identity probe marker type changed; refusing to remove it".to_owned()),
        Err(error) => failures.push(format!("inspect shared identity probe marker: {error}")),
    }
    if let Err(error) = provider.delete(world_id) {
        failures.push(error.to_string());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(failures.join("; "))
    }
}

fn run_guest(
    transport: &dyn GuestTransport,
    executable: &str,
    args: &[&str],
    deadline: Instant,
    action: &str,
) -> Result<()> {
    let output = transport
        .run(
            &RunRequest {
                executable,
                args,
                stdin: None,
                deadline,
            },
            &mut std::io::sink(),
        )
        .map_err(anyhow::Error::msg)?;
    if output.exit_code != 0 {
        bail!(
            "{action}: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.diagnostic_tail).trim()
        );
    }
    Ok(())
}

fn validate_marker(path: &Path, expected_contents: &[u8]) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect host-visible identity probe {}", path.display()))?;
    let kind = if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    let contents = fs::read(path)
        .with_context(|| format!("read host-visible identity probe {}", path.display()))?;
    validate_marker_details(
        kind,
        metadata.uid(),
        metadata.gid(),
        metadata.mode() & 0o7777,
        &contents,
        expected_contents,
    )
}

fn validate_marker_details(
    kind: &str,
    uid: u32,
    gid: u32,
    mode: u32,
    contents: &[u8],
    expected_contents: &[u8],
) -> Result<()> {
    if kind == "file"
        && uid == wt_guest::GUEST_UID
        && gid == wt_guest::GUEST_GID
        && mode == 0o600
        && contents == expected_contents
    {
        return Ok(());
    }
    bail!(
        "host/guest shared identity probe mismatch: expected file uid/gid={}:{} mode=0600 content={:?}; actual type={kind} uid/gid={uid}:{gid} mode={mode:04o} content={:?}",
        wt_guest::GUEST_UID,
        wt_guest::GUEST_GID,
        String::from_utf8_lossy(expected_contents),
        String::from_utf8_lossy(contents),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_virtiofs_probe_requires_the_canonical_identity() {
        validate_marker_details(
            "file",
            wt_guest::GUEST_UID,
            wt_guest::GUEST_GID,
            0o600,
            b"wt-identity-probe\n",
            b"wt-identity-probe\n",
        )
        .unwrap();

        let error = validate_marker_details(
            "file",
            1001,
            1000,
            0o600,
            b"wt-identity-probe\n",
            b"wt-identity-probe\n",
        )
        .unwrap_err();
        insta::assert_snapshot!(error.to_string(), @r#"host/guest shared identity probe mismatch: expected file uid/gid=1001:1001 mode=0600 content="wt-identity-probe\n"; actual type=file uid/gid=1001:1000 mode=0600 content="wt-identity-probe\n""#);
    }
}
