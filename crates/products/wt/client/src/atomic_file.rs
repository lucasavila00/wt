use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

pub(super) fn replace(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().context("atomic write target has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).context("create temporary file")?;
    temporary
        .as_file_mut()
        .write_all(contents)
        .and_then(|()| temporary.as_file().sync_all())
        .with_context(|| format!("write temporary file for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically update {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Barrier;

    #[test]
    fn concurrent_replacements_publish_one_complete_private_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        let barrier = Barrier::new(16);
        let writes = (0..16)
            .map(|index| format!("writer {index}\n").repeat(1_000))
            .collect::<Vec<_>>();
        std::thread::scope(|scope| {
            let threads = writes
                .iter()
                .map(|contents| {
                    scope.spawn(|| {
                        barrier.wait();
                        replace(&path, contents.as_bytes())
                    })
                })
                .collect::<Vec<_>>();
            for thread in threads {
                thread.join().unwrap().unwrap();
            }
        });

        let published = fs::read_to_string(&path).unwrap();
        assert!(writes.contains(&published));
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
