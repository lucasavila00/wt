use anyhow::{bail, Context, Result};
use nix::unistd::Uid;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wt_control_protocol::{ApiError, ApiProgress, ApiRequest, ApiResponse, ErrorCode};

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
             check `systemctl status wts.service` and `journalctl -u wts.service`"
        ),
        std::io::ErrorKind::PermissionDenied => anyhow::anyhow!(
            "permission denied connecting to wt-server daemon at {path}: {error}\n\
             run the command as the user that owns wts.service and {path}"
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
            .name("wt-control-protocol".to_owned())
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
            let mut progress = ProgressWriter::new(&mut stream);
            handler(request, &mut progress)
        }
        Err(error) => ApiResponse::error(ApiError::new(
            ErrorCode::InvalidRequest,
            format!("invalid JSON request: {error}"),
        )),
    };
    serde_json::to_writer(&mut stream, &response).context("encode API response")?;
    stream.write_all(b"\n").context("finish API response")?;
    Ok(())
}

struct ProgressWriter<'a> {
    output: &'a mut dyn Write,
    pending: Vec<u8>,
    disconnected: bool,
}

impl<'a> ProgressWriter<'a> {
    fn new(output: &'a mut dyn Write) -> Self {
        Self {
            output,
            pending: Vec::new(),
            disconnected: false,
        }
    }

    fn emit(&mut self, line: &[u8]) -> std::io::Result<()> {
        let message = String::from_utf8_lossy(line).trim().to_owned();
        if message.is_empty() {
            return Ok(());
        }
        eprintln!("wt-server: world creation progress: {message}");
        if self.disconnected {
            return Ok(());
        }
        let result = serde_json::to_writer(&mut self.output, &ApiProgress::new(message))
            .map_err(std::io::Error::other)
            .and_then(|()| self.output.write_all(b"\n"))
            .and_then(|()| self.output.flush());
        if result.is_err() {
            self.disconnected = true;
        }
        Ok(())
    }
}

impl Write for ProgressWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.disconnected {
            return Ok(bytes.len());
        }
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = self.pending.drain(..=end).collect::<Vec<_>>();
            self.emit(&line)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.emit(&line)?;
        }
        if self.disconnected {
            Ok(())
        } else {
            if self.output.flush().is_err() {
                self.disconnected = true;
            }
            Ok(())
        }
    }
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
    use std::io::{BufRead, BufReader};
    use wt_control_protocol::{ApiRequest, ApiResponse, Operation, Response};

    struct Disconnected;

    impl Write for Disconnected {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn disconnected_progress_consumer_does_not_fail_the_worker() {
        let mut output = Disconnected;
        let mut progress = ProgressWriter::new(&mut output);

        assert_eq!(
            progress.write(b"creating disk\n").unwrap(),
            b"creating disk\n".len()
        );
        assert_eq!(
            progress.write(b"starting guest\n").unwrap(),
            b"starting guest\n".len()
        );
        progress.flush().unwrap();
    }

    #[test]
    fn one_connection_carries_one_request_and_response() {
        let (client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|request, _| {
                assert!(matches!(request.operation, Operation::ListWorlds));
                ApiResponse::ok(Response::Worlds {
                    worlds: vec![],
                    capacity: Default::default(),
                    disk_usage_bytes: Default::default(),
                    agent_tool_report_counts: Default::default(),
                })
            })
            .unwrap();
        });
        serde_json::to_writer(&client, &ApiRequest::new(Operation::ListWorlds)).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let response: ApiResponse = serde_json::from_reader(client).unwrap();
        let wt_control_protocol::Outcome::Ok { response } = response.outcome else {
            panic!("expected successful response");
        };
        let Response::Worlds { worlds, .. } = *response else {
            panic!("expected worlds response");
        };
        assert!(worlds.is_empty());
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
        let response: ApiResponse = serde_json::from_reader(client).unwrap();
        assert!(matches!(
            response.outcome,
            wt_control_protocol::Outcome::Error { error } if error.code == ErrorCode::InvalidRequest
        ));
        thread.join().unwrap();
    }

    #[test]
    fn progress_precedes_the_final_response() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            handle_stream(server, &|_, progress| {
                writeln!(progress, "Waiting for the guest transport...").unwrap();
                ApiResponse::ok(Response::Worlds {
                    worlds: vec![],
                    capacity: Default::default(),
                    disk_usage_bytes: Default::default(),
                    agent_tool_report_counts: Default::default(),
                })
            })
            .unwrap();
        });
        serde_json::to_writer(&mut client, &ApiRequest::new(Operation::ListWorlds)).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let lines = BufReader::new(client)
            .lines()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(lines.len(), 2);
        let progress: ApiProgress = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(
            progress.event,
            wt_control_protocol::ProgressEvent::Progress {
                message: "Waiting for the guest transport...".into()
            }
        );
        let response: ApiResponse = serde_json::from_str(&lines[1]).unwrap();
        assert!(matches!(
            response.outcome,
            wt_control_protocol::Outcome::Ok { .. }
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

        insta::assert_snapshot!(
            error.replace(&temp.path().display().to_string(), "[TEMP]"),
            @r###"
            wt-server daemon is unavailable at [TEMP]/missing.sock: No such file or directory (os error 2)
            check `systemctl status wts.service` and `journalctl -u wts.service`
            "###
        );
    }
}
