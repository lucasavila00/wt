//! QEMU guest-agent implementation of the provider-neutral guest transport.

use super::lookup_domain;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::io::Write;
use std::time::{Duration, Instant};
use uuid::Uuid;
use virt::domain::Domain;
use wt_provider::{
    validate_executable, validate_file_path, CaptureRequest, CapturedOutput, GuestTransport,
    ProviderId, RunOutput, RunRequest, StreamKind, TransportError, WriteFileRequest,
};

const OUTPUT_TAIL_LIMIT: usize = 64 * 1024;
const FILE_READ_SIZE: usize = 48 * 1024;
const FILE_WRITE_SIZE: usize = 48 * 1024;
const EXEC_POLL_DELAYS_MS: [u64; 5] = [50, 100, 200, 400, 500];

#[derive(Clone, Debug)]
pub(super) struct QemuGuestTransport {
    provider_id: ProviderId,
}

impl QemuGuestTransport {
    pub(super) fn new(provider_id: ProviderId) -> Self {
        Self { provider_id }
    }

    fn domain(&self) -> Result<Domain, TransportError> {
        lookup_domain(&self.provider_id)
            .map_err(|error| TransportError::Transport(error.to_string()))
    }
}

impl GuestTransport for QemuGuestTransport {
    fn run(
        &self,
        request: &RunRequest<'_>,
        destination: &mut dyn Write,
    ) -> Result<RunOutput, TransportError> {
        validate_executable(request.executable)?;
        require_time(request.deadline)?;
        let domain = self.domain()?;
        let log_path = format!("/run/wt-command-{}.log", Uuid::new_v4());
        write_bytes(&domain, &log_path, b"")?;
        let handle = open_file(&domain, &log_path, "r")?;
        let result = (|| {
            let script =
                "log=$1; shift; \"$@\" >\"$log\" 2>&1; status=$?; rm -f -- \"$log\"; exit \"$status\"";
            let mut shell_args = vec![
                "-c",
                script,
                "wt-command",
                log_path.as_str(),
                request.executable,
            ];
            shell_args.extend_from_slice(request.args);
            let pid = start_exec(&domain, "/bin/sh", &shell_args, request.stdin)?;
            let mut tail = TailBuffer::default();
            let mut poll = PollBackoff::default();
            loop {
                let exit_code = exec_status(&domain, pid)?;
                drain_stream(&domain, handle, destination, &mut tail)?;
                if let Some(exit_code) = exit_code {
                    return Ok(RunOutput {
                        exit_code,
                        diagnostic_tail: tail.into_bytes(),
                    });
                }
                require_time(request.deadline)?;
                clear_file_eof(&domain, handle)?;
                poll.sleep();
            }
        })();
        close_file(&domain, handle);
        if result.is_err() {
            remove_guest_files(&domain, &[&log_path]);
        }
        result
    }

    fn capture(&self, request: &CaptureRequest<'_>) -> Result<CapturedOutput, TransportError> {
        validate_executable(request.executable)?;
        require_time(request.deadline)?;
        let domain = self.domain()?;
        let token = Uuid::new_v4();
        let stdout_path = format!("/run/wt-capture-{token}.stdout");
        let stderr_path = format!("/run/wt-capture-{token}.stderr");
        write_bytes(&domain, &stdout_path, b"")?;
        write_bytes(&domain, &stderr_path, b"")?;
        let stdout_handle = open_file(&domain, &stdout_path, "r")?;
        let stderr_handle = match open_file(&domain, &stderr_path, "r") {
            Ok(handle) => handle,
            Err(error) => {
                close_file(&domain, stdout_handle);
                remove_guest_files(&domain, &[&stdout_path, &stderr_path]);
                return Err(error);
            }
        };
        let result = (|| {
            let script = "stdout=$1; stderr=$2; shift 2; \"$@\" >\"$stdout\" 2>\"$stderr\"";
            let mut shell_args = vec![
                "-c",
                script,
                "wt-capture",
                stdout_path.as_str(),
                stderr_path.as_str(),
                request.executable,
            ];
            shell_args.extend_from_slice(request.args);
            let pid = start_exec(&domain, "/bin/sh", &shell_args, request.stdin)?;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut poll = PollBackoff::default();
            loop {
                let exit_code = exec_status(&domain, pid)?;
                drain_bounded(
                    &domain,
                    stdout_handle,
                    &mut stdout,
                    request.stdout_limit,
                    StreamKind::Stdout,
                )?;
                drain_bounded(
                    &domain,
                    stderr_handle,
                    &mut stderr,
                    request.stderr_limit,
                    StreamKind::Stderr,
                )?;
                if let Some(exit_code) = exit_code {
                    return Ok(CapturedOutput {
                        exit_code,
                        stdout,
                        stderr,
                    });
                }
                require_time(request.deadline)?;
                clear_file_eof(&domain, stdout_handle)?;
                clear_file_eof(&domain, stderr_handle)?;
                poll.sleep();
            }
        })();
        close_file(&domain, stdout_handle);
        close_file(&domain, stderr_handle);
        remove_guest_files(&domain, &[&stdout_path, &stderr_path]);
        result
    }

    fn write_file(&self, request: &WriteFileRequest<'_>) -> Result<(), TransportError> {
        validate_file_path(request.path)?;
        require_time(request.deadline)?;
        let domain = self.domain()?;
        write_bytes(&domain, request.path, request.contents)?;
        let owner = format!("{}:{}", request.owner, request.group);
        let mode = format!("{:04o}", request.mode);
        let chown = run_direct(
            &domain,
            "/bin/chown",
            &[&owner, request.path],
            request.deadline,
        )?;
        if chown != 0 {
            return Err(TransportError::Transport(format!(
                "set guest file ownership: exit code {chown}"
            )));
        }
        let chmod = run_direct(
            &domain,
            "/bin/chmod",
            &[&mode, request.path],
            request.deadline,
        )?;
        if chmod != 0 {
            return Err(TransportError::Transport(format!(
                "set guest file mode: exit code {chmod}"
            )));
        }
        Ok(())
    }
}

fn run_direct(
    domain: &Domain,
    executable: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<i64, TransportError> {
    require_time(deadline)?;
    let pid = start_exec(domain, executable, args, None)?;
    let mut poll = PollBackoff::default();
    loop {
        if let Some(exit_code) = exec_status(domain, pid)? {
            return Ok(exit_code);
        }
        require_time(deadline)?;
        poll.sleep();
    }
}

fn remove_guest_files(domain: &Domain, paths: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut args = vec!["-f", "--"];
    args.extend_from_slice(paths);
    let _ = run_direct(domain, "/bin/rm", &args, deadline);
}

fn start_exec(
    domain: &Domain,
    path: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<u64, TransportError> {
    let mut arguments = serde_json::json!({
        "path": path,
        "arg": args,
        "capture-output": false,
    });
    if let Some(stdin) = stdin {
        arguments["input-data"] = serde_json::Value::String(BASE64.encode(stdin));
    }
    let request = serde_json::json!({
        "execute": "guest-exec",
        "arguments": arguments,
    });
    let response = agent_command(domain, &request, "start guest command")?;
    response["return"]["pid"].as_u64().ok_or_else(|| {
        TransportError::Transport("guest agent did not return an execution pid".to_owned())
    })
}

fn exec_status(domain: &Domain, pid: u64) -> Result<Option<i64>, TransportError> {
    let request = serde_json::json!({
        "execute": "guest-exec-status",
        "arguments": { "pid": pid }
    });
    let response = agent_command(domain, &request, "read guest command")?;
    let result = &response["return"];
    Ok((result["exited"].as_bool() == Some(true))
        .then(|| result["exitcode"].as_i64().unwrap_or(-1)))
}

fn open_file(domain: &Domain, path: &str, mode: &str) -> Result<i64, TransportError> {
    let request = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": { "path": path, "mode": mode }
    });
    let response = agent_command(domain, &request, "open guest file")?;
    response["return"].as_i64().ok_or_else(|| {
        TransportError::Transport("guest agent did not return a file handle".to_owned())
    })
}

fn drain_stream(
    domain: &Domain,
    handle: i64,
    destination: &mut dyn Write,
    tail: &mut TailBuffer,
) -> Result<(), TransportError> {
    loop {
        let chunk = read_file_chunk(domain, handle)?;
        if !chunk.bytes.is_empty() {
            destination
                .write_all(&chunk.bytes)
                .map_err(|error| TransportError::LogSink(error.to_string()))?;
            destination
                .flush()
                .map_err(|error| TransportError::LogSink(error.to_string()))?;
            tail.push(&chunk.bytes);
        }
        if chunk.eof || chunk.bytes.is_empty() {
            return Ok(());
        }
    }
}

fn drain_bounded(
    domain: &Domain,
    handle: i64,
    destination: &mut Vec<u8>,
    limit: usize,
    stream: StreamKind,
) -> Result<(), TransportError> {
    loop {
        let chunk = read_file_chunk(domain, handle)?;
        append_bounded(destination, &chunk.bytes, limit, stream)?;
        if chunk.eof || chunk.bytes.is_empty() {
            return Ok(());
        }
    }
}

fn append_bounded(
    destination: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    stream: StreamKind,
) -> Result<(), TransportError> {
    if destination.len().saturating_add(chunk.len()) > limit {
        return Err(TransportError::Overflow { stream, limit });
    }
    destination.extend_from_slice(chunk);
    Ok(())
}

struct FileChunk {
    bytes: Vec<u8>,
    eof: bool,
}

fn read_file_chunk(domain: &Domain, handle: i64) -> Result<FileChunk, TransportError> {
    let request = serde_json::json!({
        "execute": "guest-file-read",
        "arguments": { "handle": handle, "count": FILE_READ_SIZE }
    });
    let response = agent_command(domain, &request, "read guest command output")?;
    let result = &response["return"];
    Ok(FileChunk {
        bytes: decode_data(result.get("buf-b64"))?,
        eof: result["eof"].as_bool() == Some(true),
    })
}

fn clear_file_eof(domain: &Domain, handle: i64) -> Result<(), TransportError> {
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

fn write_bytes(domain: &Domain, path: &str, contents: &[u8]) -> Result<(), TransportError> {
    let handle = open_file(domain, path, "w")?;
    let result = (|| {
        for chunk in contents.chunks(FILE_WRITE_SIZE) {
            let request = serde_json::json!({
                "execute": "guest-file-write",
                "arguments": { "handle": handle, "buf-b64": BASE64.encode(chunk) }
            });
            agent_command(domain, &request, "write guest file")?;
        }
        Ok(())
    })();
    close_file(domain, handle);
    result
}

fn agent_command(
    domain: &Domain,
    request: &serde_json::Value,
    action: &str,
) -> Result<serde_json::Value, TransportError> {
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| TransportError::Transport(format!("{action}: {error}")))?;
    serde_json::from_str(&response)
        .map_err(|error| TransportError::Transport(format!("decode {action}: {error}")))
}

fn decode_data(value: Option<&serde_json::Value>) -> Result<Vec<u8>, TransportError> {
    let Some(value) = value.and_then(serde_json::Value::as_str) else {
        return Ok(Vec::new());
    };
    BASE64
        .decode(value)
        .map_err(|error| TransportError::Transport(format!("decode guest command output: {error}")))
}

fn require_time(deadline: Instant) -> Result<(), TransportError> {
    if Instant::now() >= deadline {
        Err(TransportError::Deadline)
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct PollBackoff {
    index: usize,
}

impl PollBackoff {
    fn next_delay(&mut self) -> Duration {
        let delay = EXEC_POLL_DELAYS_MS[self.index.min(EXEC_POLL_DELAYS_MS.len() - 1)];
        self.index = self.index.saturating_add(1);
        Duration::from_millis(delay)
    }

    fn sleep(&mut self) {
        std::thread::sleep(self.next_delay());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_polling_backs_off_to_500_milliseconds() {
        let mut poll = PollBackoff::default();
        let delays = (0..7)
            .map(|_| poll.next_delay().as_millis())
            .collect::<Vec<_>>();
        assert_eq!(delays, [50, 100, 200, 400, 500, 500, 500]);
    }

    #[test]
    fn rolling_tail_keeps_only_the_last_64_kib() {
        let mut tail = TailBuffer::default();
        tail.push(&vec![b'a'; OUTPUT_TAIL_LIMIT]);
        tail.push(b"last");
        let bytes = tail.into_bytes();
        assert_eq!(bytes.len(), OUTPUT_TAIL_LIMIT);
        assert_eq!(&bytes[bytes.len() - 4..], b"last");
    }

    #[test]
    fn capture_limits_are_enforced_before_appending_a_chunk() {
        let mut output = b"1234".to_vec();
        let error = append_bounded(&mut output, b"56", 5, StreamKind::Stdout).unwrap_err();
        assert_eq!(
            error,
            TransportError::Overflow {
                stream: StreamKind::Stdout,
                limit: 5,
            }
        );
        assert_eq!(output, b"1234");
    }
}
