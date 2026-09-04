use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

const RPC_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn state_dir() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
            .join(".local/state/wt/codex"),
    )
}

pub(crate) fn endpoint() -> Result<String> {
    Ok(format!(
        "unix://{}",
        state_dir()?.join("app-server.sock").display()
    ))
}

pub(crate) trait Rpc {
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}

pub(crate) struct Connection {
    socket: WebSocket<UnixStream>,
    next_id: u64,
}

impl Connection {
    pub(crate) fn open() -> Result<Self> {
        let stream = UnixStream::connect(state_dir()?.join("app-server.sock"))
            .context("connect to wt-codex-app-server.service")?;
        stream.set_read_timeout(Some(RPC_TIMEOUT))?;
        stream.set_write_timeout(Some(RPC_TIMEOUT))?;
        let (socket, _) = tungstenite::client("ws://localhost/", stream)
            .context("handshake with Codex app server")?;
        let mut connection = Self { socket, next_id: 1 };
        connection.call(
            "initialize",
            json!({ "clientInfo": {
                "name": "wt", "title": "WT", "version": env!("CARGO_PKG_VERSION")
            }}),
        )?;
        connection.write(&json!({ "method": "initialized", "params": {} }))?;
        Ok(connection)
    }

    fn write(&mut self, value: &Value) -> Result<()> {
        self.socket
            .send(Message::Text(value.to_string().into()))
            .context("send Codex app-server request")
    }

    fn read(&mut self, deadline: Instant) -> Result<Value> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("Codex app-server request timed out")?;
            self.socket.get_mut().set_read_timeout(Some(remaining))?;
            match self
                .socket
                .read()
                .context("read Codex app-server response")?
            {
                Message::Text(text) => {
                    return serde_json::from_str(&text).context("decode Codex response")
                }
                Message::Close(_) => bail!("Codex app-server connection closed"),
                Message::Ping(_) => self.socket.flush().context("reply to Codex ping")?,
                _ => {}
            }
        }
    }
}

impl Rpc for Connection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({ "method": method, "id": id, "params": params }))?;
        let deadline = Instant::now() + RPC_TIMEOUT;
        loop {
            let message = self.read(deadline)?;
            if message.get("method").is_some() {
                if let Some(request_id) = message.get("id") {
                    // Never mistake a server-initiated request for our RPC response.
                    self.write(&json!({ "id": request_id, "error": {
                        "code": -32601, "message": "WT does not support interactive client requests"
                    }}))?;
                }
                // Notifications are hints; the recovery worker reconciles thread/read.
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                bail!("Codex app-server {method}: {}", error);
            }
            return message
                .get("result")
                .cloned()
                .context("Codex response has no result");
        }
    }
}
