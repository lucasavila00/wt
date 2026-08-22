use super::{load_install_input, validate_agent_tools_files};
use anyhow::{bail, Result};
use std::path::Path;

pub(crate) fn validate(input_path: &Path) -> Result<()> {
    let (input, _, _) = load_install_input(input_path)?;
    validate_agent_tools_files(&input)
}

pub(crate) fn validate_e2e(input_path: &Path) -> Result<()> {
    let (input, _, _) = load_install_input(input_path)?;
    if !input.test_server {
        bail!(
            "refusing destructive E2E setup: {} has test_server = false",
            input_path.display()
        );
    }
    validate_agent_tools_files(&input)
}
