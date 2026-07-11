use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LocalConfig {
    pub state_dir: PathBuf,
}

impl LocalConfig {
    pub fn from_env() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        let state_dir = std::env::var_os("WT_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_STATE_HOME").map(|path| PathBuf::from(path).join("wt"))
            })
            .unwrap_or_else(|| home.join(".local/state/wt"));
        Ok(Self { state_dir })
    }

    pub fn database_path(&self) -> PathBuf {
        self.state_dir.join("instances.db")
    }
}
