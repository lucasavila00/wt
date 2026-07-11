use anyhow::{bail, Context as _, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use wt_api::{ApiRequest, ApiResponse, Outcome, Response, PROTOCOL_VERSION};

pub fn call(request: &ApiRequest) -> Result<Response> {
    let mut child = Command::new("wt-local")
        .arg("api")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Provisioning progress belongs on stderr so stdout remains the JSON protocol.
        .stderr(Stdio::inherit())
        .spawn()
        .context("start wt-local helper")?;
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
