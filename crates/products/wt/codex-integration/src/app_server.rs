use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(crate) trait Rpc {
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
    fn notification(&mut self) -> Result<Value>;
}

pub(crate) struct Connection {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Connection {
    pub(crate) fn open(codex: &OsStr) -> Result<Self> {
        ensure_daemon(codex)?;
        let mut child = Command::new(codex)
            .args(["app-server", "proxy"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("connect to the Codex app server")?;
        let input = child.stdin.take().context("open Codex app-server input")?;
        let output = child
            .stdout
            .take()
            .map(BufReader::new)
            .context("open Codex app-server output")?;
        let mut connection = Self {
            child,
            input,
            output,
            next_id: 1,
        };
        connection.call(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "wt",
                    "title": "WT",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        connection.write(&json!({ "method": "initialized", "params": {} }))?;
        Ok(connection)
    }

    fn write(&mut self, value: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.input, value).context("encode Codex app-server request")?;
        self.input
            .write_all(b"\n")
            .and_then(|()| self.input.flush())
            .context("send Codex app-server request")
    }

    fn read(&mut self) -> Result<Value> {
        let mut line = String::new();
        if self
            .output
            .read_line(&mut line)
            .context("read Codex app-server response")?
            == 0
        {
            bail!("Codex app-server connection closed")
        }
        serde_json::from_str(&line).context("decode Codex app-server response")
    }
}

impl Rpc for Connection {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({ "method": method, "id": id, "params": params }))?;
        loop {
            let message = self.read()?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown app-server error");
                bail!("Codex app-server {method}: {detail}")
            }
            return message
                .get("result")
                .cloned()
                .context("Codex app-server response has no result");
        }
    }

    fn notification(&mut self) -> Result<Value> {
        loop {
            let message = self.read()?;
            if message.get("id").is_none() && message.get("method").is_some() {
                return Ok(message);
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ensure_daemon(codex: &OsStr) -> Result<()> {
    let output = Command::new(codex)
        .args(["app-server", "daemon", "start"])
        .output()
        .context("start the Codex app-server daemon")?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "start the Codex app-server daemon: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}
