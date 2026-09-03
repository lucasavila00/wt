use super::*;
use std::io::{BufRead, BufReader};
use wt_control_protocol::{ApiProgress, ProgressEvent};

#[derive(Debug)]
enum Frame {
    Progress(String),
    Response(ApiResponse),
}

pub fn call_outcome_with_progress(
    context: &Context,
    request: &ApiRequest,
    progress: impl FnMut(String),
) -> std::result::Result<Outcome, ContextError> {
    Ok(call_response_with_progress(context, request, progress)?.outcome)
}

pub fn call_response_with_progress(
    context: &Context,
    request: &ApiRequest,
    mut progress: impl FnMut(String),
) -> std::result::Result<ApiResponse, ContextError> {
    let request = serde_json::to_vec(request).map_err(|error| {
        context_error(
            context,
            "could not encode the API request",
            Some(error.to_string()),
            retry_hint(context),
        )
    })?;
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
    if let Err(error) = child
        .stdin
        .as_mut()
        .expect("piped helper stdin is available")
        .write_all(&request)
    {
        drop(child.stdin.take());
        let _ = child.kill();
        let _ = child.wait();
        return Err(context_error(
            context,
            "could not send the API request",
            Some(error.to_string()),
            retry_hint(context),
        ));
    }
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
    let mut stream_error = None;
    for line in BufReader::new(stdout).split(b'\n') {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                stream_error = Some(context_error(
                    context,
                    "could not read the context helper response",
                    Some(error.to_string()),
                    retry_hint(context),
                ));
                break;
            }
        };
        if line.is_empty() {
            continue;
        }
        if stream_error.is_some() {
            continue;
        }
        match decode_frame(context, &line).and_then(|frame| {
            accept_frame(&mut response, frame).map_err(|detail| {
                context_error(
                    context,
                    "invalid context helper response stream",
                    Some(detail.into()),
                    retry_hint(context),
                )
            })
        }) {
            Ok(Some(message)) => progress(message),
            Ok(None) => {}
            Err(error) => stream_error = Some(error),
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
    if let Some(error) = stream_error {
        return Err(error);
    }
    let response = response.ok_or_else(|| {
        context_error(
            context,
            "context helper returned no response",
            None,
            retry_hint(context),
        )
    })?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(protocol_version_error(context, response.protocol_version));
    }
    Ok(response)
}

fn accept_frame(
    response: &mut Option<ApiResponse>,
    frame: Frame,
) -> std::result::Result<Option<String>, &'static str> {
    match (response.is_some(), frame) {
        (false, Frame::Progress(message)) => Ok(Some(message)),
        (false, Frame::Response(frame)) => {
            *response = Some(frame);
            Ok(None)
        }
        (true, Frame::Progress(_) | Frame::Response(_)) => {
            Err("the final response must be the last and only terminal frame")
        }
    }
}

fn decode_frame(context: &Context, line: &[u8]) -> std::result::Result<Frame, ContextError> {
    let value: serde_json::Value =
        serde_json::from_slice(line).map_err(|error| invalid_response(context, error, line))?;
    if value.get("event").is_some() {
        let fields_are_valid = value.as_object().is_some_and(|object| {
            object.len() == 3
                && object.contains_key("protocol_version")
                && object.contains_key("event")
                && object.contains_key("message")
        });
        if !fields_are_valid {
            return Err(context_error(
                context,
                "invalid context helper response",
                Some("progress frames require exactly protocol_version, event, and message".into()),
                retry_hint(context),
            ));
        }
        let event: ApiProgress = serde_json::from_value(value)
            .map_err(|error| invalid_response(context, error, line))?;
        if event.protocol_version != PROTOCOL_VERSION {
            return Err(protocol_version_error(context, event.protocol_version));
        }
        let ProgressEvent::Progress { message } = event.event;
        Ok(Frame::Progress(message))
    } else {
        let response: ApiResponse = serde_json::from_value(value)
            .map_err(|error| invalid_response(context, error, line))?;
        if response.protocol_version != PROTOCOL_VERSION {
            return Err(protocol_version_error(context, response.protocol_version));
        }
        Ok(Frame::Response(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            name: "local".into(),
            kind: ContextKind::BareMetalLocal,
        }
    }

    #[test]
    fn frame_kind_is_explicit_and_malformed_events_are_not_responses() {
        assert!(matches!(
            decode_frame(
                &context(),
                br#"{"protocol_version":19,"event":"progress","message":"waiting"}"#,
            )
            .unwrap(),
            Frame::Progress(message) if message == "waiting"
        ));
        assert!(decode_frame(
            &context(),
            br#"{"protocol_version":19,"event":"future","message":"waiting"}"#,
        )
        .is_err());
    }

    #[test]
    fn frame_rejects_a_wrong_protocol_version() {
        let error = decode_frame(
            &context(),
            br#"{"protocol_version":7,"event":"progress","message":"waiting"}"#,
        )
        .unwrap_err();

        assert!(error.body().contains("expected 19"));
    }

    #[test]
    fn terminal_response_is_unique_and_last() {
        let response_line = br#"{"protocol_version":19,"outcome":"ok","response":{"response":"worlds","worlds":[],"disk_usage_bytes":{},"agent_tool_report_counts":{}}}"#;
        let mut response = None;
        assert!(accept_frame(
            &mut response,
            decode_frame(&context(), response_line).unwrap()
        )
        .is_ok());
        assert!(accept_frame(
            &mut response,
            decode_frame(
                &context(),
                br#"{"protocol_version":19,"event":"progress","message":"late"}"#,
            )
            .unwrap()
        )
        .is_err());

        let mut response = None;
        accept_frame(
            &mut response,
            decode_frame(&context(), response_line).unwrap(),
        )
        .unwrap();
        assert!(accept_frame(
            &mut response,
            decode_frame(&context(), response_line).unwrap()
        )
        .is_err());
    }
}
