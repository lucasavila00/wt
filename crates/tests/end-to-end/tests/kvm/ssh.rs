use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) struct AuthorizedKeysFixture {
    source: PathBuf,
    original: Vec<u8>,
}

impl AuthorizedKeysFixture {
    pub(crate) fn install(public_key: &Path) -> Self {
        let source = Path::new(wt_server::SERVER_HOME).join(".ssh/authorized_keys");
        let original = fs::read(&source).unwrap();
        let mut installed = original.clone();
        if !installed.ends_with(b"\n") {
            installed.push(b'\n');
        }
        installed.extend(fs::read(public_key).unwrap());
        if !installed.ends_with(b"\n") {
            installed.push(b'\n');
        }
        let fixture = Self { source, original };
        replace(&fixture.source, &installed).unwrap();
        wait_for_export(&installed).unwrap();
        fixture
    }
}

impl Drop for AuthorizedKeysFixture {
    fn drop(&mut self) {
        if let Err(error) =
            replace(&self.source, &self.original).and_then(|()| wait_for_export(&self.original))
        {
            eprintln!("KVM cleanup: restore host SSH authorized keys: {error}");
        }
    }
}

fn replace(path: &Path, contents: &[u8]) -> Result<(), String> {
    let mut replacement = tempfile::NamedTempFile::new_in(path.parent().unwrap())
        .map_err(|error| error.to_string())?;
    replacement
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    replacement
        .write_all(contents)
        .map_err(|error| error.to_string())?;
    replacement
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    replacement
        .persist(path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

fn wait_for_export(expected: &[u8]) -> Result<(), String> {
    let export = Path::new(wt_server::SSH_AUTHORIZED_KEYS_SHARE_DIR).join("authorized_keys");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if fs::read(&export)
            .map(|contents| contents == expected)
            .unwrap_or(false)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("SSH authorized keys export did not refresh".to_owned());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
