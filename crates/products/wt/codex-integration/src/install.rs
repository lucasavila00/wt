use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG: &str = r#"approval_policy = "never"
sandbox_mode = "danger-full-access"

[[hooks.SessionStart]]

[[hooks.SessionStart.hooks]]
type = "command"
command = '''wtg tools world-prompt'''
"#;

pub(crate) fn install_user_config() -> Result<()> {
    install_config(&codex_home()?)
}

fn codex_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CODEX_HOME") {
        return Ok(path.into());
    }
    let home = env::var_os("HOME").context("neither CODEX_HOME nor HOME is set")?;
    Ok(Path::new(&home).join(".codex"))
}

fn install_config(codex_home: &Path) -> Result<()> {
    let path = codex_home.join("config.toml");
    match fs::read(&path) {
        Ok(contents) if contents == CONFIG.as_bytes() => return Ok(()),
        Ok(_) => bail!(
            "Codex configuration differs from WT's configuration: {}",
            path.display()
        ),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error)
                .with_context(|| format!("read Codex configuration {}", path.display()));
        }
        Err(_) => {}
    }
    fs::create_dir_all(codex_home)
        .with_context(|| format!("create Codex directory {}", codex_home.display()))?;
    fs::write(&path, CONFIG)
        .with_context(|| format!("write Codex configuration {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_install_is_idempotent_and_rejects_other_contents() {
        let temp = tempdir().unwrap();
        let codex_home = temp.path().join(".codex");

        install_config(&codex_home).unwrap();
        insta::assert_snapshot!(
            fs::read_to_string(codex_home.join("config.toml")).unwrap(),
            @r###"
        approval_policy = "never"
        sandbox_mode = "danger-full-access"

        [[hooks.SessionStart]]

        [[hooks.SessionStart.hooks]]
        type = "command"
        command = '''wtg tools world-prompt'''
        "###
        );
        install_config(&codex_home).unwrap();

        fs::write(codex_home.join("config.toml"), "model = \"other\"\n").unwrap();
        let error = install_config(&codex_home)
            .unwrap_err()
            .to_string()
            .replace(&codex_home.display().to_string(), "<CODEX_HOME>");
        insta::assert_snapshot!(error, @"Codex configuration differs from WT's configuration: <CODEX_HOME>/config.toml");
    }
}
