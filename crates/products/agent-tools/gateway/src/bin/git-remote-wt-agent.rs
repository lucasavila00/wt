use anyhow::{bail, Context, Result};
use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, GitService, TransportResponse,
    PROTOCOL_VERSION,
};

const SOCKET: &str = "/run/wt-agent-tool-gateway/gateway.sock";

#[allow(dead_code)]
fn main() {
    if let Err(error) = run_from(std::env::args().skip(1)) {
        eprintln!("git-remote-wt-agent: {error:#}");
        std::process::exit(1);
    }
}

pub fn run_from(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut args = args.into_iter();
    let _remote = args.next().context("missing remote name")?;
    let source = args.next().context("missing remote URL")?;
    if args.next().is_some() {
        bail!("too many arguments");
    }
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut command = String::new();
    input
        .read_line(&mut command)
        .context("read Git helper command")?;
    if command.trim_end() != "capabilities" {
        bail!("Git did not request helper capabilities");
    }
    writeln!(output, "connect")?;
    writeln!(output)?;
    output.flush()?;
    command.clear();
    input
        .read_line(&mut command)
        .context("read Git connect command")?;
    let service = command
        .trim_end()
        .strip_prefix("connect ")
        .context("Git did not request a connect service")?;
    let service = GitService::try_from(service).map_err(anyhow::Error::msg)?;
    let socket = test_socket();
    let mut relay = UnixStream::connect(&socket).context("connect to WT Git relay")?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::Git { service, source },
        },
    )?;
    let response: TransportResponse = read_json_line(&mut relay)?;
    if !response.ok {
        bail!(
            "{}",
            response
                .error
                .as_deref()
                .unwrap_or("Git operation rejected")
        );
    }
    if let Some(message) = response.message {
        eprint!("{message}");
    }
    writeln!(output)?;
    output.flush()?;
    drop(input);
    drop(output);
    copy_stdio(relay)
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

fn copy_stdio(mut relay: UnixStream) -> Result<()> {
    let mut relay_read = relay.try_clone().context("clone relay socket")?;
    let output = std::thread::spawn(move || -> std::io::Result<()> {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = std::io::Read::read(&mut relay_read, &mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            stdout.write_all(&buffer[..count])?;
            stdout.flush()?;
        }
    });
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut stdin, &mut buffer)?;
        if count == 0 {
            break;
        }
        relay.write_all(&buffer[..count])?;
    }
    let _ = relay.shutdown(std::net::Shutdown::Write);
    output
        .join()
        .map_err(|_| anyhow::anyhow!("Git response thread panicked"))??;
    Ok(())
}
