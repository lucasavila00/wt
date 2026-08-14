use super::guest;
use crate::{GuestTransport, WorkerError};
use std::time::{Duration, Instant};

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
    }
}
