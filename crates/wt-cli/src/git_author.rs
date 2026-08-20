use anyhow::Result;
use std::process::Command;

#[derive(Debug)]
pub(super) struct GitAuthor {
    pub(super) name: String,
    pub(super) email: String,
}

pub(super) fn read_git_author() -> Result<GitAuthor> {
    Ok(GitAuthor {
        name: read_global_git_config("user.name")?,
        email: read_global_git_config("user.email")?,
    })
}

fn read_global_git_config(key: &str) -> Result<String> {
    match Command::new("git")
        .args(["config", "--global", "--null", "--get", key])
        .output()
    {
        Ok(output) if output.status.success() => parse_git_config_value(&output.stdout)?
            .ok_or_else(|| required_git_config_error(key, None)),
        Ok(output) if output.status.code() == Some(1) => Err(required_git_config_error(key, None)),
        Ok(output) => Err(required_git_config_error(
            key,
            Some(String::from_utf8_lossy(&output.stderr).trim()),
        )),
        Err(error) => Err(required_git_config_error(key, Some(&error.to_string()))),
    }
}

pub(super) fn required_git_config_error(key: &str, detail: Option<&str>) -> anyhow::Error {
    let detail = detail
        .filter(|detail| !detail.is_empty())
        .map(|detail| format!(": {detail}"))
        .unwrap_or_default();
    anyhow::anyhow!(
        "global Git {key} is required; configure it with `git config --global {key} VALUE`{detail}"
    )
}

pub(super) fn parse_git_config_value(stdout: &[u8]) -> Result<Option<String>> {
    let value = stdout.strip_suffix(b"\0").unwrap_or(stdout);
    let value = std::str::from_utf8(value).map_err(|error| anyhow::anyhow!(error))?;
    Ok((!value.is_empty()).then(|| value.to_owned()))
}
