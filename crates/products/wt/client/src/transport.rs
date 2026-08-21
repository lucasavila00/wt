use crate::config::{Context, ContextKind};
use std::fmt::Write as _;
use std::io::Write;
use std::process::{Command, Stdio};
use wt_control_protocol::{ApiError, ApiRequest, ApiResponse, Outcome, Response, PROTOCOL_VERSION};
use crate::cmd;

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
    let mut command = helper_command(context);
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
    let output = child.wait_with_output().map_err(|error| {
        context_error(
            context,
            "could not wait for the context helper",
            Some(error.to_string()),
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
            "unsupported protocol version 3; expected 2",
        );
        insta::assert_snapshot!(rejection(&context, &error).diagnostic("error"), @r###"
        error: context lab could not be queried: server rejected the request
          unsupported protocol: unsupported protocol version 3; expected 2
          hint: install protocol-compatible `wt` and `wt-server` versions on wt-lab
        "###);
    }
}
