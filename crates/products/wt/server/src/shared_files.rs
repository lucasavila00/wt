use crate::{CodexPaths, SERVER_GID, SERVER_HOME, SERVER_UID};
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SSH_AUTHORIZED_KEYS: &str = "/home/wt/.ssh/authorized_keys";
const SSH_AUTHORIZED_KEYS_SHARE: &str = "/home/wt/.ssh/.wt-authorized-keys";

pub fn publish_and_watch(codex: CodexPaths) -> Result<()> {
    let publications = [
        Publication {
            label: "Codex authentication",
            source: PathBuf::from(codex.auth),
            share: PathBuf::from(codex.auth_share),
            destination: PathBuf::from(codex.auth_share).join("auth.json"),
            nonempty: false,
            validate_ssh_keys: false,
            normalize_source_mode: true,
        },
        Publication {
            label: "SSH authorized keys",
            source: PathBuf::from(SSH_AUTHORIZED_KEYS),
            share: PathBuf::from(SSH_AUTHORIZED_KEYS_SHARE),
            destination: PathBuf::from(SSH_AUTHORIZED_KEYS_SHARE).join("authorized_keys"),
            nonempty: true,
            validate_ssh_keys: true,
            normalize_source_mode: false,
        },
    ];
    for publication in &publications {
        publication.publish()?;
    }
    std::thread::Builder::new()
        .name("wt-shared-file-publisher".to_owned())
        .spawn(move || loop {
            for publication in &publications {
                if let Err(error) = publication.publish_if_changed() {
                    eprintln!("wt-server: publish {}: {error:#}", publication.label);
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        })
        .context("start shared file publisher")?;
    Ok(())
}

struct Publication {
    label: &'static str,
    source: PathBuf,
    share: PathBuf,
    destination: PathBuf,
    nonempty: bool,
    validate_ssh_keys: bool,
    normalize_source_mode: bool,
}

impl Publication {
    fn publish_if_changed(&self) -> Result<()> {
        if fs::read(&self.source).ok() == fs::read(&self.destination).ok() {
            return Ok(());
        }
        self.publish()
    }

    fn publish(&self) -> Result<()> {
        validate_source(self)?;
        validate_share(&self.share)?;
        if self.normalize_source_mode {
            fs::set_permissions(&self.source, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("protect {}", self.source.display()))?;
        }
        if self.validate_ssh_keys {
            let status = std::process::Command::new("/usr/bin/ssh-keygen")
                .args(["-l", "-f"])
                .arg(&self.source)
                .status()
                .with_context(|| format!("validate {}", self.source.display()))?;
            if !status.success() {
                bail!("{} does not contain valid SSH public keys", self.source.display());
            }
        }
        loop {
            let contents = fs::read(&self.source)
                .with_context(|| format!("read {}", self.source.display()))?;
            let temporary = self
                .share
                .join(format!(".{}.wt-new-{}", self.label.replace(' ', "-"), std::process::id()));
            match fs::remove_file(&temporary) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("remove stale shared-file temporary"),
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .with_context(|| format!("create {}", temporary.display()))?;
            file.write_all(&contents)
                .with_context(|| format!("write {}", temporary.display()))?;
            file.sync_all()
                .with_context(|| format!("sync {}", temporary.display()))?;
            fs::rename(&temporary, &self.destination)
                .with_context(|| format!("publish {}", self.destination.display()))?;
            if fs::read(&self.source).ok().as_deref() == Some(contents.as_slice()) {
                return Ok(());
            }
        }
    }
}

fn validate_source(publication: &Publication) -> Result<()> {
    let metadata = fs::symlink_metadata(&publication.source)
        .with_context(|| format!("inspect {}", publication.source.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("{} must be a regular, non-symlink file", publication.source.display());
    }
    if metadata.uid() != SERVER_UID || metadata.gid() != SERVER_GID {
        bail!(
            "{} ownership mismatch: expected uid={SERVER_UID} gid={SERVER_GID}; actual uid={} gid={}",
            publication.source.display(),
            metadata.uid(),
            metadata.gid()
        );
    }
    if publication.nonempty && metadata.len() == 0 {
        bail!("{} must not be empty", publication.source.display());
    }
    Ok(())
}

fn validate_share(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect shared directory {}", path.display()))?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != SERVER_UID
        || metadata.gid() != SERVER_GID
        || metadata.mode() & 0o7777 != 0o700
    {
        bail!(
            "shared directory {} must be a non-symlink directory owned by {SERVER_UID}:{SERVER_GID} with mode 0700",
            path.display()
        );
    }
    Ok(())
}
