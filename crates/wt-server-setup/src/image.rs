mod builder;
mod console;
#[path = "image/host.rs"]
mod host_image;
mod recipe;

#[cfg(test)]
use console::{extract_phase_markers, progress_message, ConsoleLog};

use self::builder::*;
use self::recipe::ImageRecipe;
use crate::files::{require_named_file, require_root_file};
use crate::host;
use crate::install_input::InstallInput;
use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
#[cfg(test)]
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::Duration;
use wt_command::cmd;
use wt_devcontainer::PackageVersions;
use wt_libvirt::LIBVIRT_URI;
use wt_server::ServerConfig;

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const IMAGE_MANIFEST_VERSION: u32 = 2;
const DEVCONTAINER_IMAGE_BUILD: &[u8] =
    include_bytes!("../../../assets/world/devcontainer/build-image.sh");

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    version: u32,
    recipe_version: u32,
    source_sha256: String,
    config_sha256: String,
    inputs: BTreeMap<String, String>,
    golden_sha256: String,
    tmux_sha256: String,
    packages: PackageVersions,
    devcontainer_cli: String,
}

pub(crate) fn ensure(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    let _lock = BuildLock::acquire()?;
    require_clean_build_state(runner, server)?;
    require_clean_publication_state(server)?;
    let source = source_image(input, runner)?;
    let byobu = byobu_package(runner)?;
    let manifest_path = manifest_path(&server.image.devcontainer_path);
    match installed_image_state(
        server.image.devcontainer_path.exists(),
        manifest_path.exists(),
        || verify_installed_image(input, server, server_bytes, &manifest_path),
    ) {
        InstalledImageState::Reusable => {
            println!("Verifying installed golden image and provenance...");
            println!("Reusing verified golden image.");
        }
        InstalledImageState::Missing => {
            build_image(
                runner,
                input,
                server,
                server_bytes,
                &source,
                &byobu,
                &manifest_path,
            )?;
        }
        InstalledImageState::Replace(reason) => {
            println!("Replacing the installed devcontainer golden image: {reason}");
            println!("Existing worlds use independent disks and are unaffected.");
            build_image(
                runner,
                input,
                server,
                server_bytes,
                &source,
                &byobu,
                &manifest_path,
            )?;
        }
    }
    host_image::ensure(runner, input, server, server_bytes, &source, &byobu)
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    let _lock = BuildLock::acquire()?;
    require_clean_build_state(runner, server)?;
    require_clean_publication_state(server)?;
    let source = source_image(input, runner)?;
    let byobu = byobu_package(runner)?;
    let manifest = manifest_path(&server.image.devcontainer_path);
    build_image(
        runner,
        input,
        server,
        server_bytes,
        &source,
        &byobu,
        &manifest,
    )?;
    host_image::build(runner, input, server, server_bytes, &source, &byobu)
}

fn source_image(input: &InstallInput, runner: &impl Runner) -> Result<PathBuf> {
    let path = Path::new("imgs").join(SOURCE_IMAGE_NAME);
    fs::create_dir_all("imgs").context("create imgs directory")?;
    if path.exists() {
        println!("Verifying cached Ubuntu source image...");
        require_sha(&path, input.source_sha256(), "source image")?;
        println!("Reusing verified source image: {}", path.display());
        return Ok(path);
    }
    let temporary = path.with_extension("img.download");
    if temporary.exists() {
        bail!(
            "stale source image download exists: {}",
            temporary.display()
        );
    }
    println!("Downloading pinned Ubuntu source image...");
    runner.run(
        cmd!("curl", "-fL", "--output", &temporary, input.source_url(),),
        "download pinned Ubuntu image",
    )?;
    if let Err(error) = require_sha(&temporary, input.source_sha256(), "downloaded image") {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).context("publish source image")?;
    Ok(path)
}

fn byobu_package(runner: &impl Runner) -> Result<PathBuf> {
    let path = Path::new("imgs").join(recipe::BYOBU_DEB);
    fs::create_dir_all("imgs").context("create imgs directory")?;
    if path.exists() {
        println!("Verifying cached pinned Byobu package...");
        require_sha(&path, recipe::BYOBU_SHA256, "pinned Byobu package")?;
        println!("Reusing verified pinned Byobu package: {}", path.display());
        return Ok(path);
    }
    let temporary = path.with_extension("deb.download");
    if temporary.exists() {
        bail!(
            "stale pinned Byobu package download exists: {}",
            temporary.display()
        );
    }
    println!("Downloading pinned Byobu package from Ubuntu snapshot...");
    runner.run(
        cmd!("curl", "-fL", "--output", &temporary, recipe::BYOBU_URL),
        "download pinned Byobu package from Ubuntu snapshot",
    )?;
    if let Err(error) = require_sha(
        &temporary,
        recipe::BYOBU_SHA256,
        "downloaded pinned Byobu package",
    ) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).context("publish pinned Byobu package")?;
    Ok(path)
}

fn build_image(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
    byobu: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let build_dir = server.libvirt.worlds_dir.join(BUILD_NAME);

    if build_dir.exists() || domain_exists(runner, BUILD_NAME)? {
        bail!("stale image build state exists for {BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create image build directory")?;
    let context = BuildContext {
        runner,
        input,
        server,
        source,
        byobu,
    };
    let result = (|| {
        fs::set_permissions(&build_dir, fs::Permissions::from_mode(0o2770))
            .context("set image build directory permissions")?;
        host::ensure_qemu_search_acl(runner, &build_dir)?;
        build_image_inner(&context, server_bytes, manifest_path, &build_dir)
    })();
    if let Err(primary) = result {
        let primary = attach_console_tail(primary, &build_dir);
        return match cleanup_failed_build(runner, &build_dir, BUILD_NAME) {
            Ok(()) => Err(primary),
            Err(cleanup) => {
                Err(primary.context(format!("image build cleanup also failed: {cleanup}")))
            }
        };
    }
    Ok(())
}

fn build_image_inner<R: Runner>(
    context: &BuildContext<'_, R>,
    server_bytes: &[u8],
    manifest_path: &Path,
    build_dir: &Path,
) -> Result<()> {
    let runner = context.runner;
    let input = context.input;
    let server = context.server;
    let recipe = ImageRecipe::new();
    let spec = BuildSpec {
        name: BUILD_NAME,
        kind: "devcontainer",
        recipe_version: recipe::RECIPE_VERSION,
        recipe: DEVCONTAINER_IMAGE_BUILD,
    };
    let paths = run_kvm_build(context, build_dir, &spec, &[])?;
    let package_output = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-image-packages",
        ),
        "read installed guest package versions",
    )?;
    let packages = recipe.parse_package_versions(&package_output)?;
    let package_summary = packages
        .iter()
        .map(|(name, version)| format!("{name}={version}"))
        .collect::<Vec<_>>()
        .join(", ");
    println!("Verified packages: {package_summary}");

    let tmux_sha256 = finalize_reusable_image(runner, &paths)?;
    let tmux_version = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-tmux-version",
        ),
        "read pinned tmux version after image sysprep",
    )?;
    if tmux_version.trim() != format!("tmux {}", recipe::TMUX_VERSION) {
        bail!(
            "golden image has unexpected tmux version: {:?}",
            tmux_version.trim()
        );
    }
    let finalized_package_output = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-image-packages"
        ),
        "revalidate finalized guest package versions",
    )?;
    let finalized_packages = recipe.parse_package_versions(&finalized_package_output)?;
    if finalized_packages != packages {
        bail!("finalized image package versions changed during sanitization");
    }
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    runner.run(
        cmd!(
            "sudo",
            "chown",
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()),
            &paths.disk,
        ),
        "restore image build disk ownership",
    )?;
    println!("Compacting golden image...");
    runner.run(
        cmd!(
            "qemu-img",
            "convert",
            "-p",
            "-O",
            "qcow2",
            &paths.disk,
            &paths.prepared,
        ),
        "compact golden image",
    )?;
    runner.run(
        cmd!("qemu-img", "check", &paths.prepared),
        "check golden image",
    )?;

    println!("Hashing and publishing golden image...");
    let manifest = ImageManifest {
        version: IMAGE_MANIFEST_VERSION,
        recipe_version: recipe::RECIPE_VERSION,
        source_sha256: input.source_sha256().to_ascii_lowercase(),
        config_sha256: image_config_sha(server_bytes, input),
        inputs: staged_input_hashes(&spec, &[]),
        golden_sha256: sha_file(&paths.prepared)?,
        tmux_sha256,
        packages,
        devcontainer_cli: recipe.devcontainer_cli_version().to_owned(),
    };
    let publication = stage_publication(
        runner,
        &paths.prepared,
        &server.image.devcontainer_path,
        manifest_path,
        &manifest,
    )?;
    fs::remove_dir_all(&paths.dir).context("remove image build directory")?;
    publication.publish(runner)?;
    Ok(())
}

pub(crate) fn verify_installed_image(
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    let recipe = ImageRecipe::new();
    require_named_file(
        &server.image.devcontainer_path,
        "libvirt-qemu",
        "kvm",
        0o644,
    )?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read image manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse image manifest {}", manifest_path.display()))?;
    if manifest.version != IMAGE_MANIFEST_VERSION
        || manifest.recipe_version != recipe::RECIPE_VERSION
        || manifest.source_sha256 != input.source_sha256().to_ascii_lowercase()
        || manifest.config_sha256 != image_config_sha(server_bytes, input)
        || manifest.inputs
            != staged_input_hashes(
                &BuildSpec {
                    name: BUILD_NAME,
                    kind: "devcontainer",
                    recipe_version: recipe::RECIPE_VERSION,
                    recipe: DEVCONTAINER_IMAGE_BUILD,
                },
                &[],
            )
        || manifest.devcontainer_cli != recipe.devcontainer_cli_version()
        || !is_sha256(&manifest.tmux_sha256)
    {
        bail!("provenance does not match the current source or install input");
    }
    recipe
        .validate_package_versions(&manifest.packages)
        .context("installed image package provenance differs")?;
    require_sha(
        &server.image.devcontainer_path,
        &manifest.golden_sha256,
        "installed golden image",
    )
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum InstalledImageState {
    Missing,
    Reusable,
    Replace(String),
}

pub(super) fn installed_image_state(
    image_exists: bool,
    manifest_exists: bool,
    verify: impl FnOnce() -> Result<()>,
) -> InstalledImageState {
    match (image_exists, manifest_exists) {
        (false, false) => InstalledImageState::Missing,
        (true, true) => match verify() {
            Ok(()) => InstalledImageState::Reusable,
            Err(error) => InstalledImageState::Replace(format!("{error:#}")),
        },
        _ => InstalledImageState::Replace(
            "the image and provenance manifest are not a complete pair".to_owned(),
        ),
    }
}

pub(crate) fn manifest_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.manifest.json", image.display()))
}

pub(crate) fn sibling_temporary(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .context("installed path has no file name")?
        .to_string_lossy();
    Ok(path.with_file_name(format!(".{name}.wt-new")))
}

pub(crate) fn require_sha(path: &Path, expected: &str, description: &str) -> Result<()> {
    let actual = sha_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("{description} SHA-256 mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

pub(crate) fn sha_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).with_context(|| format!("hash {}", path.display()))?;
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
