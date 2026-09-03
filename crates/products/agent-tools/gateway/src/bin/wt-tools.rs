use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, TransportResponse,
    PROTOCOL_VERSION,
};

const SOCKET: &str = "/run/wt-agent-tool-gateway/gateway.sock";

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
    let mut relay = UnixStream::connect(&socket).with_context(|| {
        format!(
            "cannot reach the WT Git relay at {socket}; this command only works inside a running WT environment"
        )
    })?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation,
        },
    )
    .context("send command to the WT Git relay")?;
    let response: TransportResponse = read_json_line(&mut relay)
        .context("read the WT Git gateway response; the relay or gateway may have stopped")?;
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

fn request_operation(args: Vec<String>) -> ClientOperation {
    match wt_tools::WtToolsCommand::parse(&args) {
        Ok(wt_tools::WtToolsCommand::World { command }) => ClientOperation::SendMessageToParent {
            message: command.parent_message().to_owned(),
        },
        Ok(_) | Err(_) => ClientOperation::Cli { args },
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

    #[test]
    fn renders_json_errors() {
        insta::assert_snapshot!(render_error("gateway rejected command"), @r###"
        {"error":{"message":"gateway rejected command"}}
        "###);
    }

    #[test]
    fn routes_parent_messages_without_changing_other_commands() {
        assert_eq!(
            request_operation(vec![
                r#"{"command":{"action":"send_message_to_parent","message":"done"}}"#.into(),
            ]),
            ClientOperation::SendMessageToParent {
                message: "done".into(),
            }
        );
        let report =
            r#"{"command":{"action":"report_wt_tool_bug","description":"broken"}}"#.to_owned();
        assert_eq!(
            request_operation(vec![report.clone()]),
            ClientOperation::Cli { args: vec![report] }
        );
    }

    #[test]
    fn reads_a_json_command_from_a_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            "{\"command\":{\"action\":\"report_wt_tool_bug\",\"description\":\"quotes: \\\" and newlines\\n\"}}\n",
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
            vec!["{\"command\":{\"action\":\"report_wt_tool_bug\",\"description\":\"quotes: \\\" and newlines\\n\"}}\n"]
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
                r#"{"command":{"action":"report_wt_tool_issue","description":"piped input"}}"#
                    .as_bytes();

            assert_eq!(
                input_args(args, &mut stdin).unwrap(),
                vec![
                    r#"{"command":{"action":"report_wt_tool_issue","description":"piped input"}}"#
                ]
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
