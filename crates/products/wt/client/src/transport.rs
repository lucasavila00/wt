use crate::cmd;
use crate::config::{Context, ContextKind};
use std::fmt::Write as _;
use std::io::{Read as _, Write};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, Outcome, Response, PROTOCOL_VERSION};

#[derive(Debug)]
pub struct ContextError {
    pub context: String,
    summary: String,
    detail: Option<String>,
    hint: String,
}

impl ContextError {
    fn body(&self) -> String {
        let mut output = format!(
            "context {} could not be queried: {}\n",
            self.context, self.summary
        );
        if let Some(detail) = &self.detail {
            for line in detail.lines() {
                writeln!(output, "  {line}").expect("writing to a String cannot fail");
            }
        }
        write!(output, "  hint: {}", self.hint).expect("writing to a String cannot fail");
        output
    }

    pub fn diagnostic(&self, level: &str) -> String {
        let body = self.body();
        let mut lines = body.lines();
        let mut output = format!("{level}: {}\n", lines.next().unwrap_or_default());
        for line in lines {
            writeln!(output, "{line}").expect("writing to a String cannot fail");
        }
        output
    }
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.body())
    }
}

impl std::error::Error for ContextError {}

pub fn call(
    context: &Context,
    request: &ApiRequest,
) -> std::result::Result<Response, ContextError> {
    match call_outcome(context, request)? {
        Outcome::Ok { response } => Ok(*response),
        Outcome::Error { error } => Err(rejection(context, &error)),
    }
}

pub fn call_with_timeout(
    context: &Context,
    request: &ApiRequest,
    timeout: Duration,
) -> std::result::Result<Response, ContextError> {
    let cancelled = AtomicBool::new(false);
    call_with_timeout_until(context, request, timeout, &cancelled)
}

pub fn call_with_timeout_until(
    context: &Context,
    request: &ApiRequest,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> std::result::Result<Response, ContextError> {
    match call_outcome_inner(context, request, Some((timeout, cancelled)))? {
        Outcome::Ok { response } => Ok(*response),
        Outcome::Error { error } => Err(rejection(context, &error)),
    }
}

pub fn rejection(context: &Context, error: &ApiError) -> ContextError {
    let hint = match error.code {
        wt_control_protocol::ErrorCode::Capacity => {
            match error.capacity.as_ref().map(|capacity| capacity.resource) {
                Some(wt_control_protocol::CapacityResource::Cpu | wt_control_protocol::CapacityResource::Memory) => {
                    "free guest capacity with `wt stop CONTEXT.WORLD` or `wt rm CONTEXT.WORLD`, then retry".to_owned()
                }
                _ => "free guest capacity with `wt rm CONTEXT.WORLD`, then retry".to_owned(),
            }
        }
        wt_control_protocol::ErrorCode::UnsupportedProtocol => version_hint(context),
        _ => server_hint(context),
    };
    context_error(
        context,
        "server rejected the request",
        Some(format_api_error(error)),
        hint,
    )
}

pub fn call_outcome(
    context: &Context,
    request: &ApiRequest,
) -> std::result::Result<Outcome, ContextError> {
    call_outcome_inner(context, request, None)
}

fn call_outcome_inner(
    context: &Context,
    request: &ApiRequest,
    timeout: Option<(Duration, &AtomicBool)>,
) -> std::result::Result<Outcome, ContextError> {
    let mut command = helper_command(context);
    command.process_group(0);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            context_error(
                context,
                "could not start the context helper",
                Some(error.to_string()),
                start_hint(context),
            )
        })?;
    let Some(stdin) = child.stdin.as_mut() else {
        return Err(context_error(
            context,
            "context helper stdin is unavailable",
            None,
            retry_hint(context),
        ));
    };
    serde_json::to_writer(stdin, request).map_err(|error| {
        context_error(
            context,
            "could not send the API request",
            Some(error.to_string()),
            retry_hint(context),
        )
    })?;
    child
        .stdin
        .take()
        .expect("helper stdin was checked above")
        .flush()
        .map_err(|error| {
            context_error(
                context,
                "could not finish the API request",
                Some(error.to_string()),
                retry_hint(context),
            )
        })?;
    let output = wait_with_output(child, timeout).map_err(|error| {
        context_error(
            context,
            if error.kind() == std::io::ErrorKind::TimedOut {
                "context helper timed out"
            } else {
                "could not wait for the context helper"
            },
            (error.kind() != std::io::ErrorKind::TimedOut).then(|| error.to_string()),
            retry_hint(context),
        )
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(context_error(
            context,
            format!("context helper exited with {}", output.status),
            (!detail.is_empty()).then_some(detail),
            server_hint(context),
        ));
    }
    let response: ApiResponse = serde_json::from_slice(&output.stdout).map_err(|error| {
        context_error(
            context,
            "context helper returned an invalid response",
            Some(format!(
                "{error}; response: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            )),
            version_hint(context),
        )
    })?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(context_error(
            context,
            format!(
                "context helper returned protocol version {}; expected {}",
                response.protocol_version, PROTOCOL_VERSION
            ),
            None,
            version_hint(context),
        ));
    }
    Ok(response.outcome)
}

fn wait_with_output(
    mut child: std::process::Child,
    timeout: Option<(Duration, &AtomicBool)>,
) -> std::io::Result<Output> {
    let Some((timeout, cancelled)) = timeout else {
        return child.wait_with_output();
    };

    let stdout = drain_pipe(child.stdout.take());
    let stderr = drain_pipe(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let mut interrupted = None;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancelled.load(Ordering::Relaxed) {
            interrupted = Some((std::io::ErrorKind::Interrupted, "context helper cancelled"));
        } else if Instant::now() >= deadline {
            interrupted = Some((std::io::ErrorKind::TimedOut, "context helper timed out"));
        }
        if interrupted.is_some() {
            kill_process_group(&mut child);
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())));
    };
    let stdout = join_pipe(stdout)?;
    let stderr = join_pipe(stderr)?;
    if let Some((kind, message)) = interrupted {
        return Err(std::io::Error::new(kind, message));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn drain_pipe<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes)?;
        }
        Ok(bytes)
    })
}

fn join_pipe(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> std::io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| std::io::Error::other("context helper output reader panicked"))?
}

fn kill_process_group(child: &mut std::process::Child) {
    let pid = nix::unistd::Pid::from_raw(child.id().cast_signed());
    if nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGKILL).is_err() {
        let _ = child.kill();
    }
}

fn context_error(
    context: &Context,
    summary: impl Into<String>,
    detail: Option<String>,
    hint: String,
) -> ContextError {
    ContextError {
        context: context.name.clone(),
        summary: summary.into(),
        detail,
        hint,
    }
}

pub fn wrong_response(context: &Context, operation: &str) -> ContextError {
    context_error(
        context,
        format!("server returned the wrong response to {operation}"),
        None,
        version_hint(context),
    )
}

fn start_hint(context: &Context) -> String {
    match &context.kind {
        ContextKind::BareMetalLocal => {
            "verify that `wt-server` is installed and available in PATH".to_owned()
        }
        ContextKind::BareMetalSsh { host } => {
            format!("verify that OpenSSH is installed and `ssh {host}` works")
        }
    }
}

fn retry_hint(context: &Context) -> String {
    match &context.kind {
        ContextKind::BareMetalLocal => {
            "retry the command; if it fails again, check `systemctl status wt-server.service`"
                .to_owned()
        }
        ContextKind::BareMetalSsh { host } => {
            format!("retry the command; if it fails again, check `ssh {host}`")
        }
    }
}

fn server_hint(context: &Context) -> String {
    match &context.kind {
        ContextKind::BareMetalLocal => {
            "check `systemctl status wt-server.service` and `journalctl -u wt-server.service`"
                .to_owned()
        }
        ContextKind::BareMetalSsh { host } => {
            format!("check `ssh {host}` and `ssh {host} systemctl status wt-server.service`")
        }
    }
}

fn version_hint(context: &Context) -> String {
    match &context.kind {
        ContextKind::BareMetalLocal => {
            "install protocol-compatible `wt` and `wt-server` versions".to_owned()
        }
        ContextKind::BareMetalSsh { host } => {
            format!("install protocol-compatible `wt` and `wt-server` versions on {host}")
        }
    }
}

pub fn format_api_error(error: &ApiError) -> String {
    format!("{}: {}", error_code(error.code), error.message)
}

fn helper_command(context: &Context) -> Command {
    match &context.kind {
        ContextKind::BareMetalLocal => cmd!("wt-server", "api"),
        ContextKind::BareMetalSsh { host } => cmd!("ssh", "--", host, "wt-server", "api"),
    }
}

fn error_code(code: wt_control_protocol::ErrorCode) -> &'static str {
    match code {
        wt_control_protocol::ErrorCode::InvalidRequest => "invalid request",
        wt_control_protocol::ErrorCode::UnsupportedProtocol => "unsupported protocol",
        wt_control_protocol::ErrorCode::Conflict => "conflict",
        wt_control_protocol::ErrorCode::NotFound => "not found",
        wt_control_protocol::ErrorCode::Capacity => "capacity unavailable",
        wt_control_protocol::ErrorCode::Backend => "backend error",
        wt_control_protocol::ErrorCode::Internal => "internal error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::time::Instant;

    #[test]
    fn builds_exact_local_and_ssh_commands() {
        let local = Context {
            name: "local".into(),
            kind: ContextKind::BareMetalLocal,
        };
        let command = helper_command(&local);
        assert_eq!(command.get_program(), OsStr::new("wt-server"));
        assert_eq!(command.get_args().collect::<Vec<_>>(), [OsStr::new("api")]);

        let remote = Context {
            name: "lab".into(),
            kind: ContextKind::BareMetalSsh {
                host: "wt-lab".into(),
            },
        };
        let command = helper_command(&remote);
        assert_eq!(command.get_program(), OsStr::new("ssh"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--"),
                OsStr::new("wt-lab"),
                OsStr::new("wt-server"),
                OsStr::new("api")
            ]
        );
    }

    #[test]
    fn unsupported_protocol_rejection_has_a_version_hint() {
        let context = Context {
            name: "lab".into(),
            kind: ContextKind::BareMetalSsh {
                host: "wt-lab".into(),
            },
        };
        let error = ApiError::new(
            wt_control_protocol::ErrorCode::UnsupportedProtocol,
            "unsupported protocol version 4; expected 3",
        );
        insta::assert_snapshot!(rejection(&context, &error).diagnostic("error"), @r###"
        error: context lab could not be queried: server rejected the request
          unsupported protocol: unsupported protocol version 4; expected 3
          hint: install protocol-compatible `wt` and `wt-server` versions on wt-lab
        "###);
    }

    #[test]
    fn timed_wait_returns_completed_output() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf ready"]);
        command.stdout(Stdio::piped());

        let output =
            wait_with_output(command.spawn().unwrap(), Some(Duration::from_millis(500))).unwrap();

        assert_eq!(output.stdout, b"ready");
    }

    #[test]
    fn timed_wait_kills_a_slow_helper() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 10"]);
        let started = Instant::now();

        let error = wait_with_output(command.spawn().unwrap(), Some(Duration::from_millis(50)))
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
