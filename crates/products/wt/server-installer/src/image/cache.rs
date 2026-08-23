use super::builder::{self, BuildContext, BuildSource, BuildSpec};
use super::timing::{timed, TimedRunner};
use super::*;

const CACHE_SCHEMA: &str = "1";
const CACHE_INPUTS: &[&[u8]] = &[
    DEVELOPMENT_TOOLS_CACHE_BUILD,
    include_bytes!("../../../../../../assets/world/shared/install-packages.sh"),
    include_bytes!("../../../../../../assets/world/shared/install-development-tools.sh"),
    include_bytes!("../../../../../../assets/world/shared/finalize-development-tools-cache.sh"),
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DevelopmentToolsCacheManifest {
    pub(super) identity: String,
    pub(super) sha256: String,
}

pub(super) fn ensure<R: Runner>(context: &BuildContext<'_, R>) -> Result<PathBuf> {
    let directory = Path::new("imgs");
    fs::create_dir_all(directory).context("create image cache directory")?;
    let image = directory.join(DEVELOPMENT_TOOLS_CACHE_NAME);
    let manifest_path = image.with_extension("manifest.json");
    let identity = identity(
        context.input.source_sha256(),
        context.input.image.build_disk_gib,
        &guest_identity(),
        recipe::node_version(),
    );

    if image.exists() && manifest_path.exists() {
        match read_manifest(&manifest_path) {
            Ok(manifest)
                if manifest.identity == identity
                    && timed("verify cached development-tools image checksum", || {
                        require_sha(&image, &manifest.sha256, "cached development tools image")
                    })
                    .is_ok() =>
            {
                println!(
                    "Reusing verified cached development tools image: {}",
                    image.display()
                );
                return Ok(image);
            }
            Ok(_) => println!("Replacing stale cached development tools image."),
            Err(error) => println!("Replacing corrupt cached development tools image: {error:#}"),
        }
        remove_file(&image)?;
        remove_file(&manifest_path)?;
    } else if image.exists() || manifest_path.exists() {
        println!("Replacing incomplete cached development tools image.");
        remove_file(&image)?;
        remove_file(&manifest_path)?;
    }

    let temporary = image.with_extension("qcow2.new");
    remove_file(&temporary).context("remove abandoned development tools cache publication")?;
    let build_dir = context
        .server
        .libvirt
        .worlds_dir
        .join(DEVELOPMENT_TOOLS_CACHE_BUILD_NAME);
    if build_dir.exists() || domain_exists(context.runner, DEVELOPMENT_TOOLS_CACHE_BUILD_NAME)? {
        bail!("stale development tools cache build state exists for {DEVELOPMENT_TOOLS_CACHE_BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create development tools cache build directory")?;
    let result = (|| {
        fs::set_permissions(&build_dir, fs::Permissions::from_mode(0o2770))
            .context("set development tools cache build directory permissions")?;
        host::ensure_qemu_search_acl(context.runner, &build_dir)?;
        let paths = run_kvm_build(
            context,
            &build_dir,
            &BuildSpec {
                name: DEVELOPMENT_TOOLS_CACHE_BUILD_NAME,
                main_recipe: DEVELOPMENT_TOOLS_CACHE_BUILD,
                host_recipe: b"",
            },
            &[],
            BuildSource::CloudImage,
        )?;
        builder::finalize_development_tools_cache(context.runner, &paths)?;
        context.runner.timed_run(
            cmd!(
                "qemu-img",
                "convert",
                "-p",
                "-O",
                "qcow2",
                &paths.disk,
                &temporary
            ),
            "compact development-tools cache",
        )?;
        context.runner.timed_run(
            cmd!("qemu-img", "check", &temporary),
            "check development-tools cache",
        )?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))
            .context("set cached development tools image permissions")?;
        let manifest = DevelopmentToolsCacheManifest {
            identity,
            sha256: timed("hash development-tools cache", || sha_file(&temporary))?,
        };
        fs::rename(&temporary, &image).context("publish cached development tools image")?;
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
            .context("write cached development tools image manifest")?;
        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o644))
            .context("set cached development tools manifest permissions")?;
        fs::remove_dir_all(&paths.dir).context("remove development tools cache build directory")?;
        Ok(())
    })();
    if let Err(error) = result {
        let mut error = attach_console_tail(error, &build_dir);
        if let Err(cleanup) = remove_file(&temporary) {
            error = error.context(format!("remove failed cache publication: {cleanup:#}"));
        }
        return match cleanup_failed_build(
            context.runner,
            &build_dir,
            DEVELOPMENT_TOOLS_CACHE_BUILD_NAME,
        ) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(error.context(format!(
                "development tools cache cleanup also failed: {cleanup}"
            ))),
        };
    }
    println!(
        "Published cached development tools image: {}",
        image.display()
    );
    Ok(image)
}

pub(super) fn identity(
    source_sha256: &str,
    build_disk_gib: u64,
    guest_identity: &str,
    node_version: &str,
) -> String {
    let mut bytes = format!(
        "{CACHE_SCHEMA}\n{source_sha256}\n{build_disk_gib}\n{guest_identity}\n{node_version}\n"
    )
    .into_bytes();
    for content in CACHE_INPUTS {
        bytes.extend_from_slice(content);
    }
    sha_bytes(&bytes)
}

fn guest_identity() -> String {
    format!(
        "{}:{}:{}:{}:{}",
        wt_host_world::GUEST_USER,
        wt_host_world::GUEST_GROUP,
        wt_host_world::GUEST_UID,
        wt_host_world::GUEST_GID,
        wt_host_world::GUEST_HOME,
    )
}

fn read_manifest(path: &Path) -> Result<DevelopmentToolsCacheManifest> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("read cache manifest {}", path.display()))?,
    )
    .with_context(|| format!("parse cache manifest {}", path.display()))
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_cache_manifest_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("cache.manifest.json");
        fs::write(&manifest, b"not json").unwrap();

        assert!(read_manifest(&manifest).is_err());
    }

    #[test]
    fn abandoned_publication_cleanup_is_exact_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let published = directory.path().join("cache.qcow2");
        let temporary = directory.path().join("cache.qcow2.new");
        fs::write(&published, b"published").unwrap();
        fs::write(&temporary, b"temporary").unwrap();

        remove_file(&temporary).unwrap();
        remove_file(&temporary).unwrap();

        assert_eq!(fs::read(&published).unwrap(), b"published");
        assert!(!temporary.exists());
    }
}
