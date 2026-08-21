use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use wt_end_to_end_tests::cmd;

pub(crate) fn spawn_gateway(
    temp: &Path,
    binary_dir: &Path,
    api: Option<(&str, &str, &Path)>,
) -> Child {
    let control_socket = temp.join("gateway-control.sock");
    let transport_socket = temp.join("gateway-transport.sock");
    let log_path = temp.join("gateway.log");
    for socket in [&control_socket, &transport_socket] {
        match fs::remove_file(socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove stale gateway socket: {error}"),
        }
    }
    let mut gateway = cmd!(
        binary_dir.join("wt-agent-git-gateway"),
        "serve",
        "--control-socket",
        &control_socket,
        "--transport-socket",
        &transport_socket,
        "--state-file",
        temp.join("gateway-state.json"),
        "--local-provider",
        format!("local.test={}", temp.display()),
    );
    gateway.stdout(Stdio::null());
    if let Some((kind, base_url, token_file)) = api {
        gateway
            .env("WT_AGENT_GIT_TEST_PROVIDER_KIND", kind)
            .env("WT_AGENT_GIT_TEST_API_BASE", base_url)
            .env("WT_AGENT_GIT_TEST_TOKEN_FILE", token_file);
    }
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap();
    let mut gateway = gateway
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    for socket in [&control_socket, &transport_socket] {
        while !socket.exists() {
            assert_gateway_running(&mut gateway, &log_path);
            assert!(Instant::now() < deadline, "gateway socket did not appear");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_gateway_running(&mut gateway, &log_path);
    gateway
}

fn assert_gateway_running(gateway: &mut Child, log_path: &Path) {
    let Some(status) = gateway.try_wait().unwrap() else {
        return;
    };
    let log = fs::read_to_string(log_path).unwrap_or_else(|error| error.to_string());
    panic!(
        "test gateway exited during startup ({status})\n\
         gateway log ({}):\n{log}",
        log_path.display()
    );
}

pub(crate) fn spawn_provider_api_fixture(
    kind: &str,
    head: &str,
) -> (String, JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let kind = kind.to_owned();
    let head = head.to_owned();
    let fixture = std::thread::spawn(move || {
        let (stream, _) = listener
            .accept()
            .map_err(|error| format!("accept provider fixture request: {error}"))?;
        serve_provider_api_request(stream, &kind, &head)?;
        Ok(())
    });
    (base_url, fixture)
}

fn serve_provider_api_request(mut stream: TcpStream, kind: &str, head: &str) -> Result<(), String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("clone provider fixture stream: {error}"))?,
    );
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| format!("read provider fixture request: {error}"))?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("provider fixture request has no path")?
        .to_owned();
    let mut content_length = 0;
    let mut authenticated = false;
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|error| format!("read provider fixture header: {error}"))?;
        if header == "\r\n" || header.is_empty() {
            break;
        }
        let lowercase = header.to_ascii_lowercase();
        if let Some(value) = lowercase.strip_prefix("content-length:") {
            content_length = value
                .trim()
                .parse::<usize>()
                .map_err(|error| format!("parse provider fixture content length: {error}"))?;
        }
        authenticated |= if kind == "github" {
            lowercase.trim() == "authorization: bearer fixture-token"
        } else {
            lowercase.trim() == "private-token: fixture-token"
        };
    }
    if !authenticated {
        return Err("provider fixture request was not authenticated".to_owned());
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("read provider fixture body: {error}"))?;
    let response = match (kind, path.as_str()) {
        ("github", "/graphql") => r#"{"data":{"viewer":{"login":"agent"},"repository":{"id":"repository-1","nameWithOwner":"acme/widget","viewerPermission":"WRITE","pullRequests":{"pageInfo":{"hasNextPage":false},"totalCount":0,"nodes":[]}}}}"#.to_owned(),
        ("github", path) if path.starts_with("/repos/acme/widget/actions/runs?") => {
            r#"{"total_count":0,"workflow_runs":[]}"#.to_owned()
        }
        ("gitlab", "/api/graphql") => format!(
            r#"{{"data":{{"currentUser":{{"username":"agent"}},"project":{{"id":"project-1","fullPath":"acme/widget","userPermissions":{{"createMergeRequestIn":true}},"repository":{{"commit":{{"sha":"{head}"}}}},"mergeRequests":{{"pageInfo":{{"hasNextPage":false}},"nodes":[]}}}}}}}}"#
        ),
        ("gitlab", path) if path.starts_with("/api/v4/projects/acme%2Fwidget/pipelines?") => {
            "[]".to_owned()
        }
        _ => return Err(format!("unexpected {kind} fixture request path: {path}")),
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write provider fixture response: {error}"))
}
