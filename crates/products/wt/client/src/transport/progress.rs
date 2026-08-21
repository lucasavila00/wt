use super::*;
use std::io::{BufRead, BufReader};
use wt_control_protocol::{ApiProgress, ProgressEvent};

pub fn call_outcome_with_progress(
    context: &Context,
    request: &ApiRequest,
    mut progress: impl FnMut(String),
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
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .expect("piped helper stdin is available"),
        request,
    )
    .map_err(|error| {
        context_error(
            context,
            "could not send the API request",
            Some(error.to_string()),
            retry_hint(context),
        )
    })?;
    drop(child.stdin.take());
    let stderr = child
        .stderr
        .take()
        .expect("piped helper stderr is available");
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = BufReader::new(stderr).read_to_end(&mut bytes);
        bytes
    });
    let stdout = child
        .stdout
        .take()
        .expect("piped helper stdout is available");
    let mut response = None;
    for line in BufReader::new(stdout).split(b'\n') {
        let line = line.map_err(|error| {
            context_error(
                context,
                "could not read the context helper response",
                Some(error.to_string()),
                retry_hint(context),
            )
        })?;
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_slice::<ApiProgress>(&line) {
            if event.protocol_version != PROTOCOL_VERSION {
                return Err(protocol_version_error(context, event.protocol_version));
            }
            let ProgressEvent::Progress { message } = event.event;
            progress(message);
        } else {
            response = Some(line);
        }
    }
    let status = child.wait().map_err(|error| {
        context_error(
            context,
            "could not wait for the context helper",
            Some(error.to_string()),
            retry_hint(context),
        )
    })?;
    let stderr = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr).trim().to_owned();
        return Err(context_error(
            context,
            format!("context helper exited with {status}"),
            (!detail.is_empty()).then_some(detail),
            retry_hint(context),
        ));
    }
    let response = response.ok_or_else(|| {
        context_error(
            context,
            "context helper returned no response",
            None,
            retry_hint(context),
        )
    })?;
    let response: ApiResponse = serde_json::from_slice(&response)
        .map_err(|error| invalid_response(context, error, &response))?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(protocol_version_error(context, response.protocol_version));
    }
    Ok(response.outcome)
}
