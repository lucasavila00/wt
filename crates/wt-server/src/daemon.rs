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
const PROGRESS_FRAME: u8 = 1;
const RESPONSE_FRAME: u8 = 2;
const MAX_FRAME_SIZE: usize = 1024 * 1024;

pub fn proxy(
    socket_path: &Path,
    mut input: impl Read,
    mut output: impl Write,
    mut progress: impl Write,
) -> Result<()> {
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
    loop {
        let (kind, payload) = read_frame(&mut stream).context("receive API response")?;
        match kind {
            PROGRESS_FRAME => {
                progress.write_all(&payload).context("write API progress")?;
                progress.flush().context("flush API progress")?;
            }
            RESPONSE_FRAME => {
                output.write_all(&payload).context("write API response")?;
                return Ok(());
            }
            _ => bail!("unknown control frame {kind}"),
        }
    }
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
        _ => anyhow::Error::new(error).context(format!("connect to wt-server daemon at {path}")),
    }
}

pub fn serve(
    socket_path: &Path,
    handler: impl Fn(ApiRequest, &mut dyn Write) -> ApiResponse + Send + Sync + 'static,
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
    handler: &(impl Fn(ApiRequest, &mut dyn Write) -> ApiResponse + ?Sized),
) -> Result<()> {
    let mut request = Vec::new();
    stream
        .read_to_end(&mut request)
        .context("read API request")?;
    let response = match serde_json::from_slice::<ApiRequest>(&request) {
        Ok(request) => {
            let mut progress = ProgressFrames::new(&mut stream);
            handler(request, &mut progress)
        }
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    let mut encoded = serde_json::to_vec(&response).context("encode API response")?;
    encoded.push(b'\n');
    write_frame(&mut stream, RESPONSE_FRAME, &encoded).context("finish API response")?;
    Ok(())
}

struct ProgressFrames<'a> {
    stream: &'a mut UnixStream,
    connected: bool,
}

impl<'a> ProgressFrames<'a> {
    fn new(stream: &'a mut UnixStream) -> Self {
        Self {
            stream,
            connected: true,
        }
    }
}

impl Write for ProgressFrames<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.connected && write_frame(self.stream, PROGRESS_FRAME, bytes).is_err() {
            self.connected = false;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.connected && self.stream.flush().is_err() {
            self.connected = false;
        }
        Ok(())
    }
}

fn write_frame(output: &mut impl Write, kind: u8, payload: &[u8]) -> std::io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::other("control frame is too large"))?;
    output.write_all(&[kind])?;
    output.write_all(&length.to_be_bytes())?;
    output.write_all(payload)
}

fn read_frame(input: &mut impl Read) -> std::io::Result<(u8, Vec<u8>)> {
    let mut kind = [0];
    input.read_exact(&mut kind)?;
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "control frame is too large",
        ));
    }
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    Ok((kind[0], payload))
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use wt_api::{ApiRequest, ApiResponse, Operation, Response};

    #[test]
    fn proxy_separates_progress_from_the_json_response() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_stream(stream, &|_, progress| {
                progress.write_all(b"cloud-init output\n").unwrap();
                ApiResponse::ok(Response::Instances { instances: vec![] })
            })
            .unwrap();
        });
        let request = serde_json::to_vec(&ApiRequest::new(Operation::List)).unwrap();
        let mut response = Vec::new();
        let mut progress = Vec::new();

        proxy(&socket, request.as_slice(), &mut response, &mut progress).unwrap();

        assert_eq!(progress, b"cloud-init output\n");
        let response: ApiResponse = serde_json::from_slice(&response).unwrap();
        assert!(matches!(response.outcome, wt_api::Outcome::Ok { .. }));
        thread.join().unwrap();
    }

    #[test]
    fn disconnected_proxy_does_not_cancel_the_request() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let completed = Arc::new(AtomicBool::new(false));
        let handler_completed = Arc::clone(&completed);
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|_, progress| {
                progress.write_all(b"still running\n").unwrap();
                handler_completed.store(true, Ordering::SeqCst);
                ApiResponse::ok(Response::Instances { instances: vec![] })
            })
        });
        serde_json::to_writer(&mut client, &ApiRequest::new(Operation::List)).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        drop(client);

        let _ = thread.join().unwrap();
        assert!(completed.load(Ordering::SeqCst));
    }

    #[test]
    fn one_connection_carries_one_request_and_response() {
        let (client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|request, progress| {
                assert!(matches!(request.operation, Operation::List));
                progress.write_all(b"checking worlds\n").unwrap();
                ApiResponse::ok(Response::Instances { instances: vec![] })
            })
            .unwrap();
        });
        serde_json::to_writer(&client, &ApiRequest::new(Operation::List)).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let (kind, progress) = read_frame(&mut &client).unwrap();
        assert_eq!(kind, PROGRESS_FRAME);
        assert_eq!(progress, b"checking worlds\n");
        let (kind, response) = read_frame(&mut &client).unwrap();
        assert_eq!(kind, RESPONSE_FRAME);
        let response: ApiResponse = serde_json::from_slice(&response).unwrap();
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
            handle_stream(server, &|_, _| unreachable!()).unwrap();
        });
        client.write_all(b"not-json").unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let (kind, response) = read_frame(&mut &client).unwrap();
        assert_eq!(kind, RESPONSE_FRAME);
        let response: ApiResponse = serde_json::from_slice(&response).unwrap();
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
            std::io::sink(),
        )
        .unwrap_err()
        .to_string();

        insta::assert_snapshot!(
            error.replace(&temp.path().display().to_string(), "[TEMP]"),
            @r###"
            wt-server daemon is unavailable at [TEMP]/missing.sock: No such file or directory (os error 2)
            check `systemctl status wt-server.service` and `journalctl -u wt-server.service`
            "###
        );
    }
}
