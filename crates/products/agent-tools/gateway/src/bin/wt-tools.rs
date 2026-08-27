use anyhow::{bail, Context, Result};
use std::io::Write;
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
    let args = input_args(args)?;
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
            operation: ClientOperation::Cli { args },
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

fn input_args(args: Vec<String>) -> Result<Vec<String>> {
    match args.as_slice() {
        [flag, path] if flag == "--file" => {
            let command = std::fs::read_to_string(path)
                .with_context(|| format!("read JSON command file {path}"))?;
            Ok(vec![command])
        }
        [flag, ..] if flag == "--file" => bail!("usage: wtg tools --file PATH"),
        _ => Ok(args),
    }
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
    fn reads_a_json_command_from_a_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            "{\"command\":{\"action\":\"report_wt_tool_bug\",\"description\":\"quotes: \\\" and newlines\\n\"}}\n",
        )
        .unwrap();

        assert_eq!(
            input_args(vec![
                "--file".to_owned(),
                temp.path().to_str().unwrap().to_owned(),
            ])
            .unwrap(),
            vec!["{\"command\":{\"action\":\"report_wt_tool_bug\",\"description\":\"quotes: \\\" and newlines\\n\"}}\n"]
        );
    }

    #[test]
    fn file_input_requires_exactly_one_path() {
        assert!(input_args(vec!["--file".to_owned()]).is_err());
        assert!(input_args(vec![
            "--file".to_owned(),
            "command.json".to_owned(),
            "extra".to_owned()
        ])
        .is_err());
    }
}
