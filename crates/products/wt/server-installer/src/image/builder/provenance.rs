use super::*;
use crate::image::ImageManifest;
use sha2::{Digest, Sha256};
use wt_installer_support::{sudo_install, sudo_install_owned};
use wt_server::image_generation::{current_path, generations_path, manifest_path};

pub(in crate::image) struct PendingPublication {
    generation_temporary: Option<PathBuf>,
    generation_destination: PathBuf,
    staged_image: PathBuf,
    current_temporary: PathBuf,
    current_destination: PathBuf,
    link_target: PathBuf,
}

impl PendingPublication {
    pub(in crate::image) fn image_path(&self) -> &Path {
        &self.staged_image
    }

    pub(in crate::image) fn discard(self, runner: &impl Runner) -> Result<()> {
        let Some(temporary) = self.generation_temporary else {
            return Ok(());
        };
        runner.run(
            cmd!("sudo", "rm", "-rf", "--", &temporary),
            "discard staged image generation",
        )
    }

    pub(in crate::image) fn publish(self, runner: &impl Runner) -> Result<()> {
        if let Some(temporary) = self.generation_temporary {
            runner.run(
                cmd!(
                    "sudo",
                    "mv",
                    "-T",
                    "--",
                    &temporary,
                    &self.generation_destination,
                ),
                "publish complete image generation",
            )?;
        }
        runner.run(
            cmd!(
                "sudo",
                "ln",
                "-s",
                "--",
                &self.link_target,
                &self.current_temporary,
            ),
            "stage current image generation pointer",
        )?;
        runner.run(
            cmd!(
                "sudo",
                "mv",
                "-T",
                "--",
                &self.current_temporary,
                &self.current_destination,
            ),
            "publish current image generation pointer",
        )
    }
}

pub(in crate::image) fn stage_publication(
    runner: &impl Runner,
    prepared: &Path,
    image_destination: &Path,
    manifest: &ImageManifest,
) -> Result<PendingPublication> {
    wt_guest::validate_guest_identity(manifest.guest_identity).map_err(anyhow::Error::msg)?;
    let local_manifest = prepared.with_extension("manifest.json");
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    fs::write(&local_manifest, &manifest_bytes).context("write image manifest")?;
    stage_generation(
        runner,
        image_destination,
        &manifest_bytes,
        |image, installed_manifest| {
            sudo_install_owned(runner, prepared, image, "libvirt-qemu", "kvm", 0o644)?;
            crate::image::require_sha(image, &manifest.golden_sha256, "staged image generation")?;
            sudo_install(runner, &local_manifest, installed_manifest, 0o644)
        },
    )
}

fn stage_generation(
    runner: &impl Runner,
    configured_image: &Path,
    manifest_bytes: &[u8],
    stage_files: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<PendingPublication> {
    let generations = generations_path(configured_image);
    runner.run(
        cmd!(
            "sudo",
            "install",
            "-d",
            "-o",
            "root",
            "-g",
            "root",
            "-m",
            "0755",
            &generations,
        ),
        "prepare image generations directory",
    )?;
    let generation_id = sha_bytes(manifest_bytes);
    let generation_destination = generations.join(&generation_id);
    let generation_temporary = generations.join(format!(".{generation_id}.wt-new"));
    let image_name = configured_image
        .file_name()
        .context("installed image path has no file name")?;
    let installed_image = generation_temporary.join(image_name);
    let installed_manifest = manifest_path(&installed_image);
    let (generation_temporary, staged_image) = if generation_destination.exists() {
        let existing_image = generation_destination.join(image_name);
        let existing_manifest = manifest_path(&existing_image);
        if fs::read(&existing_manifest).context("read existing image generation manifest")?
            != manifest_bytes
        {
            bail!("existing image generation manifest differs");
        }
        let manifest: ImageManifest = serde_json::from_slice(manifest_bytes)?;
        crate::image::require_sha(
            &existing_image,
            &manifest.golden_sha256,
            "existing image generation",
        )?;
        (None, existing_image)
    } else {
        if generation_temporary.exists() {
            bail!("stale temporary image generation exists");
        }
        runner.run(
            cmd!(
                "sudo",
                "install",
                "-d",
                "-o",
                "root",
                "-g",
                "root",
                "-m",
                "0755",
                &generation_temporary,
            ),
            "stage image generation directory",
        )?;
        stage_files(&installed_image, &installed_manifest)?;
        (Some(generation_temporary), installed_image)
    };

    let current_destination = current_path(configured_image);
    let current_temporary = sibling_temporary(&current_destination)?;
    if fs::symlink_metadata(&current_temporary).is_ok() {
        bail!("stale temporary image generation pointer exists");
    }
    let link_target = generation_destination
        .strip_prefix(
            configured_image
                .parent()
                .context("installed image path has no parent directory")?,
        )
        .context("image generation is not beside configured image")?
        .to_path_buf();
    Ok(PendingPublication {
        generation_temporary,
        generation_destination,
        staged_image,
        current_temporary,
        current_destination,
        link_target,
    })
}

pub(in crate::image) fn sha_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::cell::Cell;
    use std::ffi::OsString;
    use std::os::unix::fs::symlink;
    use std::process::{Command, Output};
    use wt_server::image_generation::resolve;

    struct FilesystemRunner {
        fail_at: Option<usize>,
        calls: Cell<usize>,
    }

    impl Runner for FilesystemRunner {
        fn output(&self, command: Command) -> Result<Output> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            if self.fail_at == Some(call) {
                return Err(anyhow!("injected publication failure"));
            }
            let arguments = command.get_args().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(command.get_program(), "sudo");
            let mut actual = Command::new(&arguments[0]);
            if arguments[0] == "install" && arguments[1] == "-d" {
                actual.args(["-d", "-m", "0755"]);
                actual.arg(arguments.last().unwrap());
            } else {
                actual.args(&arguments[1..]);
            }
            actual.output().context("run publication test command")
        }
    }

    fn generation(directory: &Path, name: &str, image: &[u8], manifest: &[u8]) -> PathBuf {
        let path = directory.join(name);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("host.qcow2"), image).unwrap();
        fs::write(path.join("host.qcow2.manifest.json"), manifest).unwrap();
        path
    }

    fn read_pair(image: &Path) -> (Vec<u8>, Vec<u8>) {
        let resolved = resolve(image).unwrap();
        (
            fs::read(resolved.image).unwrap(),
            fs::read(resolved.manifest).unwrap(),
        )
    }

    fn publication(image: &Path, temporary: &Path, destination: &Path) -> PendingPublication {
        PendingPublication {
            generation_temporary: Some(temporary.to_path_buf()),
            generation_destination: destination.to_path_buf(),
            staged_image: temporary.join("host.qcow2"),
            current_temporary: sibling_temporary(&current_path(image)).unwrap(),
            current_destination: current_path(image),
            link_target: PathBuf::from("host.qcow2.generations/new"),
        }
    }

    #[test]
    fn failed_probe_discards_only_the_temporary_generation() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("host.qcow2");
        let generations = generations_path(&image);
        fs::create_dir(&generations).unwrap();
        let existing = generation(&generations, "existing", b"old image", b"old manifest");
        let temporary = generation(&generations, ".new.wt-new", b"new image", b"new manifest");
        let destination = generations.join("new");
        let publication = publication(&image, &temporary, &destination);
        assert_eq!(publication.image_path(), temporary.join("host.qcow2"));

        publication
            .discard(&FilesystemRunner {
                fail_at: None,
                calls: Cell::new(0),
            })
            .unwrap();

        assert!(!temporary.exists());
        assert!(existing.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn readers_see_complete_generation_across_publication_failures() {
        for failure in 1..=3 {
            let directory = tempfile::tempdir().unwrap();
            let image = directory.path().join("host.qcow2");
            let generations = generations_path(&image);
            fs::create_dir(&generations).unwrap();
            generation(&generations, "old", b"old image", b"old manifest");
            let temporary = generation(&generations, ".new.wt-new", b"new image", b"new manifest");
            let destination = generations.join("new");
            symlink("host.qcow2.generations/old", current_path(&image)).unwrap();
            let runner = FilesystemRunner {
                fail_at: Some(failure),
                calls: Cell::new(0),
            };

            assert!(publication(&image, &temporary, &destination)
                .publish(&runner)
                .is_err());
            assert_eq!(
                read_pair(&image),
                (b"old image".to_vec(), b"old manifest".to_vec())
            );
        }

        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("host.qcow2");
        let generations = generations_path(&image);
        fs::create_dir(&generations).unwrap();
        generation(&generations, "old", b"old image", b"old manifest");
        let temporary = generation(&generations, ".new.wt-new", b"new image", b"new manifest");
        let destination = generations.join("new");
        symlink("host.qcow2.generations/old", current_path(&image)).unwrap();
        let runner = FilesystemRunner {
            fail_at: None,
            calls: Cell::new(0),
        };

        publication(&image, &temporary, &destination)
            .publish(&runner)
            .unwrap();
        assert_eq!(
            read_pair(&image),
            (b"new image".to_vec(), b"new manifest".to_vec())
        );
    }

    #[test]
    fn reader_keeps_old_generation_when_manifest_staging_fails() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("host.qcow2");
        let generations = generations_path(&image);
        fs::create_dir(&generations).unwrap();
        generation(&generations, "old", b"old image", b"old manifest");
        symlink("host.qcow2.generations/old", current_path(&image)).unwrap();
        let runner = FilesystemRunner {
            fail_at: None,
            calls: Cell::new(0),
        };

        assert!(
            stage_generation(&runner, &image, b"new manifest", |staged_image, _| {
                fs::write(staged_image, b"new image")?;
                bail!("injected failure before manifest staging")
            })
            .is_err()
        );
        assert_eq!(
            read_pair(&image),
            (b"old image".to_vec(), b"old manifest".to_vec())
        );
        let temporary = fs::read_dir(&generations)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".wt-new")
            })
            .unwrap();
        assert!(temporary.join("host.qcow2").is_file());
        assert!(!temporary.join("host.qcow2.manifest.json").exists());
    }
}
