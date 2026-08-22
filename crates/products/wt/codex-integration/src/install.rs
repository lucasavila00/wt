use anyhow::{bail, Context, Result};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const REAL_CODEX_SUFFIX: &str = ".codex/packages/standalone/current/bin/codex";
const LEGACY_CONFIG: &str = r#"approval_policy = "never"
sandbox_mode = "danger-full-access"

[[hooks.SessionStart]]

[[hooks.SessionStart.hooks]]
type = "command"
command = '''wt-tools world-prompt'''
"#;
const CONFIG: &str = r#"approval_policy = "never"
sandbox_mode = "danger-full-access"

[[hooks.SessionStart]]

[[hooks.SessionStart.hooks]]
type = "command"
command = '''wt-tools world-prompt'''

[[hooks.SessionStart.hooks]]
type = "command"
command = '''wt-codex-integration report-hook'''

[[hooks.UserPromptSubmit]]

[[hooks.UserPromptSubmit.hooks]]
type = "command"
command = '''wt-codex-integration report-hook'''

[[hooks.Stop]]

[[hooks.Stop.hooks]]
type = "command"
command = '''wt-codex-integration report-hook'''

[[hooks.SessionEnd]]

[[hooks.SessionEnd.hooks]]
type = "command"
command = '''wt-codex-integration report-hook'''
"#;

pub(crate) fn invoked_as_codex(args: &[OsString]) -> Result<bool> {
    let argv0 = args.first().context("missing process name")?;
    Ok(Path::new(argv0).file_name() == Some(OsStr::new("codex")))
}

pub(crate) fn real_codex() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    real_codex_at(Path::new(&home))
}

fn real_codex_at(home: &Path) -> Result<PathBuf> {
    let path = home.join(REAL_CODEX_SUFFIX);
    match path.metadata() {
        Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => Ok(path),
        Ok(_) => bail!(
            "real Codex CLI is missing or not executable at {}; recreate this world from a verified WT image",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => bail!(
            "real Codex CLI is missing or not executable at {}; recreate this world from a verified WT image",
            path.display()
        ),
        Err(error) => Err(error).with_context(|| format!("inspect real Codex CLI {}", path.display())),
    }
}

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
        Ok(contents) if contents == LEGACY_CONFIG.as_bytes() => {}
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
    fn resolves_the_fixed_standalone_codex_path() {
        let temp = tempdir().unwrap();
        let real = temp.path().join(REAL_CODEX_SUFFIX);
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "real codex").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(real_codex_at(temp.path()).unwrap(), real);
    }

    #[test]
    fn missing_real_codex_has_an_actionable_error() {
        let temp = tempdir().unwrap();
        let error = real_codex_at(temp.path())
            .unwrap_err()
            .to_string()
            .replace(&temp.path().display().to_string(), "<HOME>");

        insta::assert_snapshot!(error, @"real Codex CLI is missing or not executable at <HOME>/.codex/packages/standalone/current/bin/codex; recreate this world from a verified WT image");
    }

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
        command = '''wt-tools world-prompt'''

        [[hooks.SessionStart.hooks]]
        type = "command"
        command = '''wt-codex-integration report-hook'''

        [[hooks.UserPromptSubmit]]

        [[hooks.UserPromptSubmit.hooks]]
        type = "command"
        command = '''wt-codex-integration report-hook'''

        [[hooks.Stop]]

        [[hooks.Stop.hooks]]
        type = "command"
        command = '''wt-codex-integration report-hook'''

        [[hooks.SessionEnd]]

        [[hooks.SessionEnd.hooks]]
        type = "command"
        command = '''wt-codex-integration report-hook'''
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

    #[test]
    fn config_install_upgrades_the_previous_wt_config() {
        let temp = tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join("config.toml"), LEGACY_CONFIG).unwrap();

        install_config(&codex_home).unwrap();

        assert_eq!(
            fs::read_to_string(codex_home.join("config.toml")).unwrap(),
            CONFIG
        );
    }
}
