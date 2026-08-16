use anyhow::{bail, Context, Result};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::Command;
use wt_agent_git::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, Repository, TransportResponse,
    PROTOCOL_VERSION,
};

const SOCKET: &str = "/run/wt-agent-git/gateway.sock";

fn main() {
    if let Err(error) = run() {
        eprintln!("ag-git: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect();
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
            operation: ClientOperation::Cli {
                args,
                repository: origin_repository(),
                branch: None,
                head: None,
            },
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

fn origin_repository() -> Option<Repository> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    repository_from_origin(std::str::from_utf8(&output.stdout).ok()?.trim())
}

fn repository_from_origin(origin: &str) -> Option<Repository> {
    let origin = origin.strip_prefix("ag::").unwrap_or(origin);
    let (host, path) = if let Some(rest) = origin.strip_prefix("https://") {
        let (host, path) = rest.split_once('/')?;
        (host, path)
    } else if let Some(rest) = origin.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        let (_, host) = authority.rsplit_once('@')?;
        (host, path)
    } else {
        let (authority, path) = origin.split_once(':')?;
        let (_, host) = authority.rsplit_once('@')?;
        (host, path)
    };
    let host = host.split_once(':').map_or(host, |(host, _)| host);
    let project = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || project.is_empty() {
        return None;
    }
    Some(Repository {
        host: host.to_owned(),
        project: project.to_owned(),
    })
}

fn test_socket() -> String {
    if cfg!(debug_assertions) {
        std::env::var("WT_AGENT_GIT_TEST_SOCKET")
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
    fn recognizes_normal_and_gateway_origins() {
        for origin in [
            "ag::git@github.com:wtco/wt.git",
            "git@github.com:wtco/wt.git",
            "ssh://git@github.com/wtco/wt.git",
            "https://github.com/wtco/wt.git",
        ] {
            assert_eq!(
                repository_from_origin(origin),
                Some(Repository {
                    host: "github.com".to_owned(),
                    project: "wtco/wt".to_owned(),
                })
            );
        }
    }
}
