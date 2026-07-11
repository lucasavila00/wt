//! Small QEMU guest-agent transport used during world provisioning.

use crate::WorkerError;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::io::Write;
use std::time::{Duration, Instant};
use uuid::Uuid;
use virt::domain::Domain;

const OUTPUT_TAIL_LIMIT: usize = 64 * 1024;
const FILE_READ_SIZE: usize = 48 * 1024;

pub(super) struct Output {
    exit_code: i64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

pub(super) fn run_phase(
    domain: &Domain,
    phase: &str,
    path: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<(), WorkerError> {
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    let output = exec_streaming(domain, path, args, deadline, &mut stderr)
        .map_err(|error| WorkerError::new(format!("{phase}: {error}")))?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "{phase}: exit code {}: {}",
            output.exit_code,
            String::from_utf8_lossy(&output.tail).trim()
        )));
    }
    Ok(())
}

pub(super) fn capture_phase(
    domain: &Domain,
    phase: &str,
    path: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<Vec<u8>, WorkerError> {
    let output = exec(domain, path, args, deadline)
        .map_err(|error| WorkerError::new(format!("{phase}: {error}")))?;
    if output.exit_code != 0 {
        return Err(WorkerError::new(format!(
            "{phase}: exit code {}: {}",
            output.exit_code,
            tail_output(&output.stdout, &output.stderr)
        )));
    }
    Ok(output.stdout)
}

struct StreamOutput {
    exit_code: i64,
    tail: Vec<u8>,
}

fn exec_streaming(
    domain: &Domain,
    path: &str,
    args: &[&str],
    deadline: Instant,
    destination: &mut impl Write,
) -> Result<StreamOutput, WorkerError> {
    if Instant::now() >= deadline {
        return Err(WorkerError::new("recipe deadline exceeded"));
    }
    let log_path = format!("/run/wt-command-{}.log", Uuid::new_v4());
    write(domain, &log_path, b"")?;
    let handle = open_file(domain, &log_path, "r")?;
    let result = (|| {
        let script =
            "log=$1; shift; \"$@\" >\"$log\" 2>&1; status=$?; rm -f -- \"$log\"; exit \"$status\"";
        let mut shell_args = vec!["-c", script, "wt-command", log_path.as_str(), path];
        shell_args.extend_from_slice(args);
        let pid = start_exec(domain, "/bin/sh", &shell_args, false)?;
        let mut tail = TailBuffer::default();

        loop {
            let exit_code = exec_status(domain, pid)?;
            drain_file(domain, handle, destination, &mut tail)?;
            if let Some(exit_code) = exit_code {
                return Ok(StreamOutput {
                    exit_code,
                    tail: tail.into_bytes(),
                });
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new("recipe deadline exceeded"));
            }
            clear_file_eof(domain, handle)?;
            std::thread::sleep(Duration::from_millis(500));
        }
    })();
    close_file(domain, handle);
    if result.is_err() {
        let _ = exec(domain, "/bin/rm", &["-f", "--", &log_path], deadline);
    }
    result
}

pub(super) fn exec(
    domain: &Domain,
    path: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<Output, WorkerError> {
    if Instant::now() >= deadline {
        return Err(WorkerError::new("recipe deadline exceeded"));
    }
    let pid = start_exec(domain, path, args, true)?;

    loop {
        if let Some(result) = read_exec_status(domain, pid)? {
            return Ok(Output {
                exit_code: result["exitcode"].as_i64().unwrap_or(-1),
                stdout: decode_data(result.get("out-data"))?,
                stderr: decode_data(result.get("err-data"))?,
            });
        }
        if Instant::now() >= deadline {
            return Err(WorkerError::new("recipe deadline exceeded"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn start_exec(
    domain: &Domain,
    path: &str,
    args: &[&str],
    capture_output: bool,
) -> Result<u64, WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-exec",
        "arguments": { "path": path, "arg": args, "capture-output": capture_output }
    });
    let response = agent_command(domain, &request, "start guest command")?;
    response["return"]["pid"]
        .as_u64()
        .ok_or_else(|| WorkerError::new("guest agent did not return an execution pid"))
}

fn exec_status(domain: &Domain, pid: u64) -> Result<Option<i64>, WorkerError> {
    Ok(read_exec_status(domain, pid)?.map(|result| result["exitcode"].as_i64().unwrap_or(-1)))
}

fn read_exec_status(domain: &Domain, pid: u64) -> Result<Option<serde_json::Value>, WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-exec-status",
        "arguments": { "pid": pid }
    });
    let response = agent_command(domain, &request, "read guest command")?;
    let result = &response["return"];
    Ok((result["exited"].as_bool() == Some(true)).then(|| result.clone()))
}

fn open_file(domain: &Domain, path: &str, mode: &str) -> Result<i64, WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": { "path": path, "mode": mode }
    });
    let response = agent_command(domain, &request, "open guest output file")?;
    response["return"]
        .as_i64()
        .ok_or_else(|| WorkerError::new("guest agent did not return a file handle"))
}

fn drain_file(
    domain: &Domain,
    handle: i64,
    destination: &mut impl Write,
    tail: &mut TailBuffer,
) -> Result<(), WorkerError> {
    loop {
        let request = serde_json::json!({
            "execute": "guest-file-read",
            "arguments": { "handle": handle, "count": FILE_READ_SIZE }
        });
        let response = agent_command(domain, &request, "read guest command output")?;
        let result = &response["return"];
        let chunk = decode_data(result.get("buf-b64"))?;
        if !chunk.is_empty() {
            forward_chunk(destination, tail, &chunk)?;
        }
        if result["eof"].as_bool() == Some(true) || chunk.is_empty() {
            return Ok(());
        }
    }
}

fn forward_chunk(
    destination: &mut impl Write,
    tail: &mut TailBuffer,
    chunk: &[u8],
) -> Result<(), WorkerError> {
    destination
        .write_all(chunk)
        .map_err(|error| error_context("forward guest command output", error))?;
    destination
        .flush()
        .map_err(|error| error_context("flush guest command output", error))?;
    tail.push(chunk);
    Ok(())
}

fn clear_file_eof(domain: &Domain, handle: i64) -> Result<(), WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-file-seek",
        "arguments": { "handle": handle, "offset": 0, "whence": "cur" }
    });
    agent_command(domain, &request, "continue reading guest command output")?;
    Ok(())
}

fn close_file(domain: &Domain, handle: i64) {
    let request = serde_json::json!({
        "execute": "guest-file-close",
        "arguments": { "handle": handle }
    });
    let _ = domain.qemu_agent_command(&request.to_string(), 10, 0);
}

fn agent_command(
    domain: &Domain,
    request: &serde_json::Value,
    action: &str,
) -> Result<serde_json::Value, WorkerError> {
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| error_context(action, error))?;
    serde_json::from_str(&response)
        .map_err(|error| error_context(&format!("decode {action}"), error))
}

/// Writes through QEMU's file API so provisioning never needs guest SSH.
pub(super) fn write(domain: &Domain, path: &str, contents: &[u8]) -> Result<(), WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": { "path": path, "mode": "w" }
    });
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| error_context("open guest file", error))?;
    let response: serde_json::Value = serde_json::from_str(&response)
        .map_err(|error| error_context("decode guest file response", error))?;
    let handle = response["return"]
        .as_i64()
        .ok_or_else(|| WorkerError::new("guest agent did not return a file handle"))?;
    let result = (|| {
        // Stay below the guest-agent message limit after base64 expansion.
        for chunk in contents.chunks(48 * 1024) {
            let request = serde_json::json!({
                "execute": "guest-file-write",
                "arguments": { "handle": handle, "buf-b64": BASE64.encode(chunk) }
            });
            domain
                .qemu_agent_command(&request.to_string(), 10, 0)
                .map_err(|error| error_context("write guest file", error))?;
        }
        Ok(())
    })();
    let close = serde_json::json!({
        "execute": "guest-file-close", "arguments": { "handle": handle }
    });
    let _ = domain.qemu_agent_command(&close.to_string(), 10, 0);
    result
}

fn decode_data(value: Option<&serde_json::Value>) -> Result<Vec<u8>, WorkerError> {
    let Some(value) = value.and_then(serde_json::Value::as_str) else {
        return Ok(Vec::new());
    };
    BASE64
        .decode(value)
        .map_err(|error| error_context("decode guest command output", error))
}

fn tail_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut combined = Vec::with_capacity(stdout.len() + stderr.len() + 1);
    combined.extend_from_slice(stdout);
    if !stdout.is_empty() && !stderr.is_empty() {
        combined.push(b'\n');
    }
    combined.extend_from_slice(stderr);
    let start = combined.len().saturating_sub(OUTPUT_TAIL_LIMIT);
    String::from_utf8_lossy(&combined[start..])
        .trim()
        .to_owned()
}

#[derive(Default)]
struct TailBuffer {
    bytes: Vec<u8>,
}

impl TailBuffer {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        let excess = self.bytes.len().saturating_sub(OUTPUT_TAIL_LIMIT);
        if excess > 0 {
            self.bytes.drain(..excess);
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn error_context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{forward_chunk, tail_output, TailBuffer, OUTPUT_TAIL_LIMIT};

    #[test]
    fn rolling_tail_keeps_only_the_last_64_kib() {
        let mut tail = TailBuffer::default();
        tail.push(&vec![b'a'; OUTPUT_TAIL_LIMIT]);
        tail.push(b"last");

        let bytes = tail.into_bytes();
        assert_eq!(bytes.len(), OUTPUT_TAIL_LIMIT);
        assert_eq!(&bytes[bytes.len() - 4..], b"last");
        assert!(bytes[..bytes.len() - 4].iter().all(|byte| *byte == b'a'));
    }

    #[test]
    fn captured_failure_tail_combines_stdout_and_stderr() {
        assert_eq!(tail_output(b"stdout", b"stderr"), "stdout\nstderr");
    }

    #[test]
    fn forwarded_chunks_are_raw_and_do_not_require_a_trailing_newline() {
        let mut destination = Vec::new();
        let mut tail = TailBuffer::default();

        forward_chunk(&mut destination, &mut tail, b"partial").unwrap();
        forward_chunk(&mut destination, &mut tail, &[0xff, b'!']).unwrap();

        assert_eq!(destination, b"partial\xff!");
        assert_eq!(tail.into_bytes(), destination);
    }
}
