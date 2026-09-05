use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Default, Deserialize, Serialize)]
pub struct State {
    pub threads: BTreeMap<String, Thread>,
    pub requests: BTreeMap<String, Receipt>,
    pub events: Vec<Value>,
    pub acknowledged: u64,
}

#[derive(Deserialize, Serialize)]
pub struct Thread {
    pub provider_id: String,
    pub turns: BTreeMap<String, Turn>,
}

#[derive(Deserialize, Serialize)]
pub struct Turn {
    pub provider_id: Option<String>,
    pub terminal: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Receipt {
    pub input: Value,
    pub result: Option<Value>,
}

pub struct Store {
    directory: PathBuf,
    _lock: File,
    pub state: State,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("state.lock"))?;
        lock.lock()?;
        let path = directory.join("state.json");
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("read agapi state")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            directory: directory.to_owned(),
            _lock: lock,
            state,
        })
    }

    pub fn save(&self) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&serde_json::to_vec(&self.state)?)?;
        file.as_file().sync_all()?;
        file.persist(self.directory.join("state.json"))?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    pub fn replay(&self, id: &str, input: &Value) -> Result<Option<Value>> {
        match self.state.requests.get(id) {
            None => Ok(None),
            Some(receipt) => {
                ensure!(
                    receipt.input == *input,
                    "request ID reused with different content"
                );
                receipt.result.clone().map(Some).context(
                    "outcome unknown: request was recorded before provider dispatch; never replay automatically"
                )
            }
        }
    }
}
