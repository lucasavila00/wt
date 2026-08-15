use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const MANAGED_INCLUDE: &str = "Include ~/.ssh/wt/config";
const MANAGED_PATH: &str = "~/.ssh/wt/config";

pub(crate) fn ensure_managed_include(path: &Path) -> Result<()> {
    let target = write_target(path)?;
    let (contents, mode) = match fs::read_to_string(&target) {
        Ok(contents) => {
            let mode = fs::metadata(&target)
                .with_context(|| format!("inspect {}", target.display()))?
                .permissions()
                .mode();
            (contents, mode)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && target == path => {
            (String::new(), 0o600)
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", target.display())),
    };
    let Some(updated) = add_managed_include(&contents) else {
        return Ok(());
    };
    atomic_write(&target, updated.as_bytes(), mode)
}

fn write_target(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::canonicalize(path)
                .with_context(|| format!("resolve symlink {}", path.display()))?;
            if !fs::metadata(&target)?.is_file() {
                bail!("{} does not point to a regular file", path.display());
            }
            Ok(target)
        }
        Ok(metadata) if metadata.is_file() => Ok(path.to_owned()),
        Ok(_) => bail!("{} is not a regular file", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_owned()),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn add_managed_include(contents: &str) -> Option<String> {
    if contents.lines().any(has_managed_include) {
        return None;
    }
    let offset = contents
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line))
        })
        .find_map(|(offset, line)| is_host_block(line).then_some(offset))
        .unwrap_or(contents.len());
    let mut updated = String::with_capacity(contents.len() + MANAGED_INCLUDE.len() + 2);
    updated.push_str(&contents[..offset]);
    if offset == contents.len() && !contents.is_empty() && !contents.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(MANAGED_INCLUDE);
    updated.push('\n');
    updated.push_str(&contents[offset..]);
    Some(updated)
}

fn has_managed_include(line: &str) -> bool {
    let mut fields = line
        .split('#')
        .next()
        .unwrap_or_default()
        .split_whitespace();
    fields
        .next()
        .is_some_and(|field| field.eq_ignore_ascii_case("include"))
        && fields.any(|field| field.trim_matches(['\'', '"']) == MANAGED_PATH)
}

fn is_host_block(line: &str) -> bool {
    line.split('#')
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .is_some_and(|field| {
            field.eq_ignore_ascii_case("host") || field.eq_ignore_ascii_case("match")
        })
}

fn atomic_write(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    let temporary = path.with_extension("wt-new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    let result = (|| {
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("atomically update {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_after_global_configuration_and_before_host_blocks() {
        let existing = "# workstation\nInclude ~/.ssh/company/config\nCanonicalizeHostname yes\n\nHost server\n  HostName server.test\n";
        let updated = add_managed_include(existing).unwrap();
        insta::assert_snapshot!(updated, @r###"
        # workstation
        Include ~/.ssh/company/config
        CanonicalizeHostname yes

        Include ~/.ssh/wt/config
        Host server
          HostName server.test
        "###);
    }

    #[test]
    fn recognizes_an_existing_include_among_other_paths() {
        let existing = "Include ~/.ssh/company/config \"~/.ssh/wt/config\"\nHost server\n";
        assert!(add_managed_include(existing).is_none());
    }

    #[test]
    fn preserves_permissions_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("dotfiles-ssh-config");
        let link = temp.path().join("config");
        fs::write(&target, "Host server\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        ensure_managed_include(&link).unwrap();
        ensure_managed_include(&link).unwrap();

        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "Include ~/.ssh/wt/config\nHost server\n"
        );
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn rejects_a_non_file_without_changing_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::create_dir(&path).unwrap();

        let error = ensure_managed_include(&path).unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("{} is not a regular file", path.display())
        );
        assert!(path.is_dir());
    }
}
