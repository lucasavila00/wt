use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

const MANAGED_INCLUDE: &str = "Include ~/.ssh/wt/config";

pub(crate) fn ensure_managed_include(path: &Path) -> Result<()> {
    match read_config(path)? {
        Some(contents) if has_leading_include(&contents) => Ok(()),
        Some(_) => bail!(
            "{} does not load WT SSH aliases before other active directives",
            path.display()
        ),
        None => create_config(path),
    }
}

fn read_config(path: &Path) -> Result<Option<String>> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("open {}", path.display())),
    };
    if !file.metadata()?.is_file() {
        bail!("{} is not a regular file", path.display());
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(Some(contents))
}

fn has_leading_include(contents: &str) -> bool {
    contents
        .lines()
        .map(|line| line.trim_matches([' ', '\t']))
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        == Some(MANAGED_INCLUDE)
}

fn create_config(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary file in {}", parent.display()))?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary.write_all(format!("{MANAGED_INCLUDE}\n").as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)
        .with_context(|| format!("create {} without replacing it", path.display()))?;
    File::open(parent)
        .with_context(|| format!("open {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("sync {}", parent.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_missing_config_once_with_private_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");

        ensure_managed_include(&path).unwrap();
        ensure_managed_include(&path).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "Include ~/.ssh/wt/config\n"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_dir(temp.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
            vec![std::ffi::OsString::from("config")]
        );
    }

    #[test]
    fn creation_never_replaces_a_concurrently_created_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(&path, "Host user-config\n").unwrap();

        assert!(create_config(&path).is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), "Host user-config\n");
    }

    #[test]
    fn accepts_a_leading_include_among_other_includes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let contents = "# workstation\n\nInclude ~/.ssh/wt/config\nInclude ~/.ssh/company/config\nHost server\n";
        fs::write(&path, contents).unwrap();

        ensure_managed_include(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    }

    #[test]
    fn rejects_scoped_or_late_includes_without_modifying_the_file() {
        for contents in [
            "Include ~/.ssh/company/config\nInclude ~/.ssh/wt/config\n",
            "Host unrelated\n  Include ~/.ssh/wt/config\n",
            "Match host unrelated\n  Include ~/.ssh/wt/config\n",
            "\u{a0}Include ~/.ssh/wt/config\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("config");
            fs::write(&path, contents).unwrap();

            assert!(ensure_managed_include(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }

    #[test]
    fn accepts_a_symlink_without_replacing_it() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("dotfiles-ssh-config");
        let link = temp.path().join("config");
        fs::write(&target, "Include ~/.ssh/wt/config\nHost server\n").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        ensure_managed_include(&link).unwrap();

        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn rejects_unreadable_config_types_without_modifying_them() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::create_dir(&path).unwrap();

        assert!(ensure_managed_include(&path).is_err());
        assert!(path.is_dir());
    }

    #[test]
    fn rejects_special_files_without_blocking_or_reading_them() {
        let temp = tempfile::tempdir().unwrap();
        let fifo = temp.path().join("fifo-config");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let device = temp.path().join("device-config");
        std::os::unix::fs::symlink("/dev/zero", &device).unwrap();

        assert!(ensure_managed_include(&fifo).is_err());
        assert!(ensure_managed_include(&device).is_err());
        assert!(fifo.exists());
        assert!(fs::symlink_metadata(device)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn rejects_non_utf8_content_without_modifying_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let contents = b"Include ~/.ssh/wt/config\n\xff";
        fs::write(&path, contents).unwrap();

        assert!(ensure_managed_include(&path).is_err());

        assert_eq!(fs::read(&path).unwrap(), contents);
    }
}
