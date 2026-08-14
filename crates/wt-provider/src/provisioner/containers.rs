use super::guest;
use crate::{GuestTransport, WorkerError};
use std::time::{Duration, Instant};

const APP_AUTHORIZED_KEYS: &str = "/var/lib/wt-app-ssh/public/authorized_keys";
const APP_AUTHORIZED_KEYS_TEMP: &str =
    "/var/lib/wt-app-ssh/public/authorized_keys/.wt-restart-authorized-keys";

pub(super) fn start_all(
    transport: &dyn GuestTransport,
    deadline: Instant,
) -> Result<(), WorkerError> {
    let ids = wait_for_docker(transport, deadline)?;
    if ids.is_empty() {
        return Err(WorkerError::new("world has no Docker containers to start"));
    }
    let mut args = vec!["start"];
    args.extend(ids.iter().map(String::as_str));
    guest::run_phase(
        transport,
        "start world containers",
        "/usr/bin/docker",
        &args,
        deadline,
        &mut std::io::sink(),
    )
}

pub(super) fn restore_app_access(
    transport: &dyn GuestTransport,
    user: &str,
    deadline: Instant,
) -> Result<(), WorkerError> {
    let workstation = guest::capture_phase(
        transport,
        "world SSH authorized keys",
        "/bin/cat",
        &["/home/wt/.ssh/authorized_keys"],
        deadline,
    )?;
    let session = guest::capture_phase(
        transport,
        "app session public key",
        "/bin/cat",
        &["/var/lib/wt-app-ssh/session_identity.pub"],
        deadline,
    )?;
    let contents = authorized_keys(&workstation, &session)?;
    guest::write_owned(
        transport,
        APP_AUTHORIZED_KEYS_TEMP,
        &contents,
        "root",
        "root",
        0o644,
        deadline,
    )?;
    let destination = format!("{APP_AUTHORIZED_KEYS}/{user}");
    guest::run_phase(
        transport,
        "restore app SSH authorized keys",
        "/bin/mv",
        &["--", APP_AUTHORIZED_KEYS_TEMP, &destination],
        deadline,
        &mut std::io::sink(),
    )
}

fn wait_for_docker(
    transport: &dyn GuestTransport,
    deadline: Instant,
) -> Result<Vec<String>, WorkerError> {
    loop {
        let output = guest::exec(
            transport,
            "/usr/bin/docker",
            &["ps", "--all", "--quiet", "--no-trunc"],
            deadline,
        )?;
        if output.exit_code == 0 {
            return container_ids(&output.stdout);
        }
        if Instant::now() >= deadline {
            return Err(WorkerError::new(format!(
                "wait for Docker after world start: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn container_ids(output: &[u8]) -> Result<Vec<String>, WorkerError> {
    output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let id = std::str::from_utf8(line).map_err(|error| {
                WorkerError::new(format!("decode Docker container ID: {error}"))
            })?;
            if id.len() != 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(WorkerError::new("Docker returned an invalid container ID"));
            }
            Ok(id.to_owned())
        })
        .collect()
}

fn authorized_keys(workstation: &[u8], session: &[u8]) -> Result<Vec<u8>, WorkerError> {
    if session.is_empty() {
        return Err(WorkerError::new("app session public key is empty"));
    }
    let mut contents = workstation.to_vec();
    if !contents.is_empty() && !contents.ends_with(b"\n") {
        contents.push(b'\n');
    }
    contents.extend_from_slice(session);
    if !contents.ends_with(b"\n") {
        contents.push(b'\n');
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_full_docker_container_ids() {
        let first = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let second = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert_eq!(
            container_ids(format!("{first}\n{second}\n").as_bytes()).unwrap(),
            [first, second]
        );
        assert!(container_ids(b"short\n").is_err());
        assert!(container_ids(
            b"ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789\n"
        )
        .is_err());
        assert!(container_ids(b"\xff\n").is_err());
    }

    #[test]
    fn assembles_complete_app_authorized_keys() {
        assert_eq!(
            authorized_keys(b"ssh-ed25519 workstation", b"ssh-ed25519 session\n").unwrap(),
            b"ssh-ed25519 workstation\nssh-ed25519 session\n"
        );
        assert_eq!(
            authorized_keys(b"", b"ssh-ed25519 session\n").unwrap(),
            b"ssh-ed25519 session\n"
        );
        assert!(authorized_keys(b"ssh-ed25519 workstation\n", b"").is_err());
    }
}
