//! Small QEMU guest-agent transport used during world provisioning.

use crate::WorkerError;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::time::{Duration, Instant};
use virt::domain::Domain;

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

pub(super) fn exec(
    domain: &Domain,
    path: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<Output, WorkerError> {
    if Instant::now() >= deadline {
        return Err(WorkerError::new("recipe deadline exceeded"));
    }
    let request = serde_json::json!({
        "execute": "guest-exec",
        "arguments": { "path": path, "arg": args, "capture-output": true }
    });
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| error_context("start guest command", error))?;
    let response: serde_json::Value = serde_json::from_str(&response)
        .map_err(|error| error_context("decode guest command response", error))?;
    let pid = response["return"]["pid"]
        .as_u64()
        .ok_or_else(|| WorkerError::new("guest agent did not return an execution pid"))?;

    loop {
        let request = serde_json::json!({
            "execute": "guest-exec-status",
            "arguments": { "pid": pid }
        });
        let response = domain
            .qemu_agent_command(&request.to_string(), 10, 0)
            .map_err(|error| error_context("read guest command", error))?;
        let response: serde_json::Value = serde_json::from_str(&response)
            .map_err(|error| error_context("decode guest command status", error))?;
        let result = &response["return"];
        if result["exited"].as_bool() == Some(true) {
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
    const LIMIT: usize = 64 * 1024;
    let mut combined = Vec::with_capacity(stdout.len() + stderr.len() + 1);
    combined.extend_from_slice(stdout);
    if !stdout.is_empty() && !stderr.is_empty() {
        combined.push(b'\n');
    }
    combined.extend_from_slice(stderr);
    let start = combined.len().saturating_sub(LIMIT);
    String::from_utf8_lossy(&combined[start..])
        .trim()
        .to_owned()
}

fn error_context(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}
