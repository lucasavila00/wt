use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, TransportResponse,
    PROTOCOL_VERSION,
};

const SOCKET: &str = "/run/wt-agent-tool-gateway/gateway.sock";
const PARENT_MESSAGE_ATTEMPTS: usize = 2;

#[allow(dead_code)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let plain_text =
        matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "help" | "--help" | "-h"));
    if let Err(error) = run(args) {
        if plain_text {
            eprintln!("wt-tools: {error:#}");
        } else {
            eprintln!("{}", render_error(&format!("{error:#}")));
        }
        std::process::exit(1);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn render_error(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": { "message": message } })
}

pub fn run(args: Vec<String>) -> Result<()> {
    let args = input_args(args, &mut std::io::stdin().lock())?;
    let operation = request_operation(args);
    let socket = test_socket();
    let response = send_operation(&socket, &operation)?;
    if !response.ok {
        bail!(
            "{}",
            response
                .error
                .as_deref()
                .unwrap_or("gateway rejected command")
        );
    }
    if let Some(message) = response.message {
        std::io::stdout()
            .write_all(message.as_bytes())
            .context("write gateway output")?;
    }
    Ok(())
}

fn send_operation(socket: &str, operation: &ClientOperation) -> Result<TransportResponse> {
    let attempts = if matches!(operation, ClientOperation::SendMessageToParent { .. }) {
        PARENT_MESSAGE_ATTEMPTS
    } else {
        1
    };
    let mut last_error = None;
    for _ in 0..attempts {
        match send_once(socket, operation) {
            Ok(response) => return Ok(response),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.expect("at least one transport attempt"))
}

fn send_once(socket: &str, operation: &ClientOperation) -> Result<TransportResponse> {
    let mut relay = UnixStream::connect(socket).with_context(|| {
        format!(
            "cannot reach the WT Git relay at {socket}; this command only works inside a running WT environment"
        )
    })?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: operation.clone(),
        },
    )
    .context("send command to the WT Git relay")?;
    read_json_line(&mut relay)
        .context("read the WT Git gateway response; the relay or gateway may have stopped")
}

fn request_operation(args: Vec<String>) -> ClientOperation {
    match wt_tools::WtToolsCommand::parse(&args) {
        Ok(wt_tools::WtToolsCommand::World { command }) => ClientOperation::SendMessageToParent {
            client_message_id: uuid::Uuid::new_v4(),
            message: command.parent_message().to_owned(),
        },
        Ok(wt_tools::WtToolsCommand::GitHosting { .. }) | Err(_) => ClientOperation::Cli { args },
    }
}

fn input_args(args: Vec<String>, stdin: &mut impl Read) -> Result<Vec<String>> {
    match args.as_slice() {
        [input] if matches!(input.as_str(), "-" | "--stdin") => read_stdin(stdin),
        [flag, path] if flag == "--file" => {
            if path == "-" {
                return read_stdin(stdin);
            }
            let command = std::fs::read_to_string(path)
                .with_context(|| format!("read JSON command file {path}"))?;
            Ok(vec![command])
        }
        [flag, ..] if flag == "--file" => bail!("usage: wtg tools --file PATH"),
        [flag, ..] if flag == "--stdin" => bail!("usage: wtg tools --stdin"),
        _ => Ok(args),
    }
}

fn read_stdin(stdin: &mut impl Read) -> Result<Vec<String>> {
    let mut command = String::new();
    stdin
        .read_to_string(&mut command)
        .context("read JSON command from standard input")?;
    Ok(vec![command])
}

fn test_socket() -> String {
    if cfg!(debug_assertions) {
        std::env::var("WT_AGENT_TOOL_TEST_SOCKET")
            .ok()
            .unwrap_or_else(|| SOCKET.to_owned())
    } else {
        SOCKET.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn renders_json_errors() {
        insta::assert_snapshot!(render_error("gateway rejected command"), @r###"
        {"error":{"message":"gateway rejected command"}}
        "###);
    }

    #[test]
    fn parent_message_gets_a_transport_retry_identity() {
        let operation = request_operation(vec![
            r#"{"command":{"action":"send_message_to_parent","message":"ready"}}"#.into(),
        ]);
        let ClientOperation::SendMessageToParent {
            client_message_id,
            message,
        } = operation
        else {
            panic!("parent message was sent as an ordinary CLI command")
        };
        assert_ne!(client_message_id, uuid::Uuid::nil());
        assert_eq!(message, "ready");
    }

    #[test]
    fn parent_message_retries_a_lost_response_with_the_same_identity() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("relay.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for attempt in 0..PARENT_MESSAGE_ATTEMPTS {
                let (mut stream, _) = listener.accept().unwrap();
                let request: ClientRequest = read_json_line(&mut stream).unwrap();
                requests.push(request);
                if attempt + 1 == PARENT_MESSAGE_ATTEMPTS {
                    write_json_line(&mut stream, &TransportResponse::ok()).unwrap();
                }
            }
            requests
        });
        let operation = request_operation(vec![
            r#"{"command":{"action":"send_message_to_parent","message":"ready"}}"#.into(),
        ]);

        assert!(
            send_operation(socket.to_str().unwrap(), &operation)
                .unwrap()
                .ok
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), PARENT_MESSAGE_ATTEMPTS);
        assert_eq!(requests[0], requests[1]);
    }

    #[test]
    fn reads_a_json_command_from_a_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            "{\"command\":{\"action\":\"send_message_to_parent\",\"message\":\"quotes: \\\" and newlines\\n\"}}\n",
        )
        .unwrap();

        assert_eq!(
            input_args(
                vec![
                    "--file".to_owned(),
                    temp.path().to_str().unwrap().to_owned(),
                ],
                &mut std::io::empty(),
            )
            .unwrap(),
            vec!["{\"command\":{\"action\":\"send_message_to_parent\",\"message\":\"quotes: \\\" and newlines\\n\"}}\n"]
        );
    }

    #[test]
    fn file_input_requires_exactly_one_path() {
        assert!(input_args(vec!["--file".to_owned()], &mut std::io::empty()).is_err());
        assert!(input_args(
            vec![
                "--file".to_owned(),
                "command.json".to_owned(),
                "extra".to_owned()
            ],
            &mut std::io::empty(),
        )
        .is_err());
    }

    #[test]
    fn reads_a_json_command_from_standard_input() {
        for args in [
            vec!["-".to_owned()],
            vec!["--stdin".to_owned()],
            vec!["--file".to_owned(), "-".to_owned()],
        ] {
            let mut stdin =
                r#"{"command":{"action":"send_message_to_parent","message":"piped input"}}"#
                    .as_bytes();

            assert_eq!(
                input_args(args, &mut stdin).unwrap(),
                vec![r#"{"command":{"action":"send_message_to_parent","message":"piped input"}}"#]
            );
        }
    }

    #[test]
    fn stdin_flag_rejects_other_arguments() {
        assert!(input_args(
            vec!["--stdin".to_owned(), "extra".to_owned()],
            &mut std::io::empty(),
        )
        .is_err());
    }
}
