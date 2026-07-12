use crate::config::{Context, ContextKind};
use anyhow::{bail, Context as _, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use wt_api::{ApiRequest, ApiResponse, Outcome, Response, PROTOCOL_VERSION};
use wt_command::cmd;

pub fn call(context: &Context, request: &ApiRequest) -> Result<Response> {
    let mut command = helper_command(context);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Provisioning progress belongs on stderr so stdout remains the JSON protocol.
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("start helper for context {}", context.name))?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .context("helper stdin is unavailable")?,
        request,
    )?;
    child.stdin.take().unwrap().flush()?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("helper exited with {}", output.status);
    }
    let response: ApiResponse = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "decode helper response: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    if response.protocol_version != PROTOCOL_VERSION {
        bail!(
            "helper returned protocol version {}; expected {}",
            response.protocol_version,
            PROTOCOL_VERSION
        );
    }
    match response.outcome {
        Outcome::Ok { response } => Ok(*response),
        Outcome::Error { error } => bail!("{}: {}", error_code(error.code), error.message),
    }
}

fn helper_command(context: &Context) -> Command {
    match &context.kind {
        ContextKind::BareMetalLocal => cmd!("wt-server", "api"),
        ContextKind::BareMetalSsh { host } => cmd!("ssh", "--", host, "wt-server", "api"),
    }
}

fn error_code(code: wt_api::ErrorCode) -> &'static str {
    match code {
        wt_api::ErrorCode::InvalidRequest => "invalid request",
        wt_api::ErrorCode::UnsupportedProtocol => "unsupported protocol",
        wt_api::ErrorCode::Conflict => "conflict",
        wt_api::ErrorCode::NotFound => "not found",
        wt_api::ErrorCode::Backend => "backend error",
        wt_api::ErrorCode::Internal => "internal error",
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
}
