use std::fmt;
use std::io::Write;
use std::time::Instant;
use thiserror::Error;

pub struct RunRequest<'a> {
    pub executable: &'a str,
    pub args: &'a [&'a str],
    pub stdin: Option<&'a [u8]>,
    pub deadline: Instant,
}

impl fmt::Debug for RunRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunRequest")
            .field("executable", &self.executable)
            .field("args", &self.args)
            .field("stdin", &self.stdin.map(|_| "<redacted>"))
            .field("deadline", &self.deadline)
            .finish()
    }
}

pub struct CaptureRequest<'a> {
    pub executable: &'a str,
    pub args: &'a [&'a str],
    pub stdin: Option<&'a [u8]>,
    pub deadline: Instant,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
}

impl fmt::Debug for CaptureRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaptureRequest")
            .field("executable", &self.executable)
            .field("args", &self.args)
            .field("stdin", &self.stdin.map(|_| "<redacted>"))
            .field("deadline", &self.deadline)
            .field("stdout_limit", &self.stdout_limit)
            .field("stderr_limit", &self.stderr_limit)
            .finish()
    }
}

pub struct WriteFileRequest<'a> {
    pub path: &'a str,
    pub contents: &'a [u8],
    pub owner: &'a str,
    pub group: &'a str,
    pub mode: u32,
    pub deadline: Instant,
}

impl fmt::Debug for WriteFileRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WriteFileRequest")
            .field("path", &self.path)
            .field("contents", &"<redacted>")
            .field("owner", &self.owner)
            .field("group", &self.group)
            .field("mode", &format_args!("{:04o}", self.mode))
            .field("deadline", &self.deadline)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunOutput {
    pub exit_code: i64,
    pub diagnostic_tail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedOutput {
    pub exit_code: i64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

impl fmt::Display for StreamKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => formatter.write_str("stdout"),
            Self::Stderr => formatter.write_str("stderr"),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransportError {
    #[error("guest transport: {0}")]
    Transport(String),
    #[error("guest command deadline exceeded")]
    Deadline,
    #[error("guest command {stream} exceeded the {limit}-byte capture limit")]
    Overflow { stream: StreamKind, limit: usize },
    #[error("forward guest command output: {0}")]
    LogSink(String),
}

pub trait GuestTransport: Send + Sync {
    fn run(
        &self,
        request: &RunRequest<'_>,
        output: &mut dyn Write,
    ) -> Result<RunOutput, TransportError>;
    fn capture(&self, request: &CaptureRequest<'_>) -> Result<CapturedOutput, TransportError>;
    fn write_file(&self, request: &WriteFileRequest<'_>) -> Result<(), TransportError>;
}

pub fn validate_executable(path: &str) -> Result<(), TransportError> {
    if path.starts_with('/')
        && path != "/"
        && !path.ends_with('/')
        && !path.contains("//")
        && !path.split('/').any(|part| matches!(part, "." | ".."))
    {
        Ok(())
    } else {
        Err(TransportError::Transport(format!(
            "guest executable must be an absolute normalized path: {path}"
        )))
    }
}

pub fn validate_file_path(path: &str) -> Result<(), TransportError> {
    validate_executable(path).map_err(|_| {
        TransportError::Transport(format!(
            "guest file path must be an absolute normalized path: {path}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn sensitive_request_debug_is_redacted() {
        let secret = b"do-not-print-this";
        let run = RunRequest {
            executable: "/usr/bin/example",
            args: &[],
            stdin: Some(secret),
            deadline: Instant::now() + Duration::from_secs(1),
        };
        let write = WriteFileRequest {
            path: "/run/example",
            contents: secret,
            owner: "root",
            group: "root",
            mode: 0o600,
            deadline: run.deadline,
        };
        assert!(!format!("{run:?}").contains("do-not-print-this"));
        assert!(!format!("{write:?}").contains("do-not-print-this"));
    }

    #[test]
    fn executable_and_file_paths_are_absolute_and_normalized() {
        assert!(validate_executable("/usr/bin/true").is_ok());
        assert!(validate_executable("usr/bin/true").is_err());
        assert!(validate_executable("/usr/../bin/true").is_err());
        assert!(validate_executable("/usr//bin/true").is_err());
        assert!(validate_executable("/usr/bin/").is_err());
        assert!(validate_file_path("/run/wt/file").is_ok());
    }
}
