use anyhow::{bail, Context, Result};
use nix::unistd::Uid;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wt_api::{ApiError, ApiRequest, ApiResponse, ErrorCode};

pub const CONTROL_SOCKET_PATH: &str = "/run/wt/server.sock";

pub fn proxy(socket_path: &Path, mut input: impl Read, mut output: impl Write) -> Result<()> {
    let mut request = Vec::new();
    input
        .read_to_end(&mut request)
        .context("read API request")?;
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| daemon_connection_error(socket_path, error))?;
    stream.write_all(&request).context("send API request")?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .context("finish API request")?;
    std::io::copy(&mut stream, &mut output).context("receive API response")?;
    Ok(())
}

fn daemon_connection_error(socket_path: &Path, error: std::io::Error) -> anyhow::Error {
    let path = socket_path.display();
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => anyhow::anyhow!(
            "wt-server daemon is unavailable at {path}: {error}\n\
             check `systemctl status wt-server.service` and `journalctl -u wt-server.service`"
        ),
        std::io::ErrorKind::PermissionDenied => anyhow::anyhow!(
            "permission denied connecting to wt-server daemon at {path}: {error}\n\
             run the command as the user that owns wt-server.service and {path}"
        ),
        _ => anyhow::Error::new(error)
            .context(format!("connect to wt-server daemon at {path}")),
    }
}

pub fn serve(
    socket_path: &Path,
    handler: impl Fn(ApiRequest) -> ApiResponse + Send + Sync + 'static,
) -> Result<()> {
    prepare_socket_path(socket_path)?;
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind control socket {}", socket_path.display()))?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))
        .context("set control socket permissions")?;
    let _socket = SocketGuard(socket_path.to_owned());
    let handler = Arc::new(handler);
    for stream in listener.incoming() {
        let stream = stream.context("accept control connection")?;
        let handler = Arc::clone(&handler);
        std::thread::Builder::new()
            .name("wt-api".to_owned())
            .spawn(move || {
                if let Err(error) = handle_stream(stream, &*handler) {
                    eprintln!("wt-server: control connection: {error:#}");
                }
            })
            .context("start control connection handler")?;
    }
    Ok(())
}

fn handle_stream(
    mut stream: UnixStream,
    handler: &(impl Fn(ApiRequest) -> ApiResponse + ?Sized),
) -> Result<()> {
    let mut request = Vec::new();
    stream
        .read_to_end(&mut request)
        .context("read API request")?;
    let response = match serde_json::from_slice::<ApiRequest>(&request) {
        Ok(request) => handler(request),
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    serde_json::to_writer(&mut stream, &response).context("encode API response")?;
    stream.write_all(b"\n").context("finish API response")?;
    Ok(())
}

fn prepare_socket_path(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        bail!("control socket has no parent directory");
    };
    if !parent.is_dir() {
        bail!(
            "control socket directory does not exist: {}",
            parent.display()
        );
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("inspect control socket"),
    };
    if !metadata.file_type().is_socket() || metadata.uid() != Uid::effective().as_raw() {
        bail!(
            "refusing to replace unexpected control socket path {}",
            path.display()
        );
    }
    if UnixStream::connect(path).is_ok() {
        bail!(
            "wt-server daemon is already listening at {}",
            path.display()
        );
    }
    fs::remove_file(path).context("remove stale control socket")
}

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt_api::{ApiRequest, ApiResponse, Operation, Response};

    #[test]
    fn one_connection_carries_one_request_and_response() {
        let (client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|request| {
                assert!(matches!(request.operation, Operation::List));
                ApiResponse::ok(Response::Instances { instances: vec![] })
            })
            .unwrap();
        });
        serde_json::to_writer(&client, &ApiRequest::new(Operation::List)).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let response: ApiResponse = serde_json::from_reader(client).unwrap();
        let wt_api::Outcome::Ok { response } = response.outcome else {
            panic!("expected successful response");
        };
        let Response::Instances { instances } = *response else {
            panic!("expected instances response");
        };
        assert!(instances.is_empty());
        thread.join().unwrap();
    }

    #[test]
    fn invalid_json_returns_a_protocol_error() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|_| unreachable!()).unwrap();
        });
        client.write_all(b"not-json").unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let response: ApiResponse = serde_json::from_reader(client).unwrap();
        assert!(matches!(
            response.outcome,
            wt_api::Outcome::Error { error } if error.code == ErrorCode::InvalidRequest
        ));
        thread.join().unwrap();
    }

    #[test]
    fn missing_daemon_socket_has_actionable_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let error = proxy(
            &temp.path().join("missing.sock"),
            std::io::empty(),
            std::io::sink(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("wt-server daemon is unavailable"));
        assert!(error.contains("systemctl status wt-server.service"));
        assert!(error.contains("journalctl -u wt-server.service"));
    }
}
