use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct StateConfig {
    pub state_dir: PathBuf,
}

impl StateConfig {
    pub fn from_env() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned())?;
        let state_dir = home.join(".local/state/wt");
        Ok(Self { state_dir })
    }

    pub fn database_path(&self) -> PathBuf {
        self.state_dir.join("instances.db")
    }
}
