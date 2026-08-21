use anyhow::Result;
use wt_client::config::ClientConfig;

mod model;

pub fn run(_config: &ClientConfig) -> Result<()> {
    anyhow::bail!("wt shell is not implemented yet")
}
