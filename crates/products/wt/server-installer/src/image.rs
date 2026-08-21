mod builder;
mod console;
mod recipe;

#[cfg(test)]
use console::{extract_phase_markers, progress_message, ConsoleLog};

use self::builder::*;
use self::recipe::ImageRecipe;
use crate::host;
use crate::install_input::InstallInput;
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
use wt_installer_support::cmd;
use wt_installer_support::{require_named_file, require_root_file, Runner};
use wt_libvirt_kvm::LIBVIRT_URI;
use recipe::PackageVersions;
#[cfg(test)]
use wt_server::image_generation::manifest_path;
use wt_server::image_generation::resolve;
use wt_server::ServerConfig;

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const RETAINED_IMAGE_BUILD: &[u8] =
    include_bytes!("../../../../../assets/world/retained/build-image.sh");
const HOST_SHELL: &[u8] = include_bytes!("../../../../../assets/world/host/shell.sh");
const HOST_PREPARE: &[u8] = include_bytes!("../../../../../assets/world/host/prepare.sh");
const HOST_INSPECT: &[u8] = include_bytes!("../../../../../assets/world/host/inspect.sh");
const HOST_CLOUD_INIT: &[u8] = include_bytes!("../../../../../assets/world/host/cloud-init.sh");
const HOST_SETUP: &[u8] = include_bytes!("../../../../../assets/world/host/setup.sh");
const HOST_DEFER_INIT: &[u8] = include_bytes!("../../../../../assets/world/host/defer-init.yaml");
const HOST_CLOUD_CONFIG: &[u8] =
    include_bytes!("../../../../../assets/world/host/cloud-config.conf");
const HOST_CLOUD_FINAL: &[u8] = include_bytes!("../../../../../assets/world/host/cloud-final.conf");
const HOST_SETUP_SERVICE: &[u8] = include_bytes!("../../../../../assets/world/host/setup.service");
const HOST_INPUTS: &[(&str, &str, &[u8])] = &[
    ("host-shell", "/var/tmp/wt-host-shell", HOST_SHELL),
    ("host-prepare", "/var/tmp/wt-host-prepare", HOST_PREPARE),
    ("host-inspect", "/var/tmp/wt-host-inspect", HOST_INSPECT),
    (
        "host-cloud-init",
        "/var/tmp/wt-host-cloud-init",
        HOST_CLOUD_INIT,
    ),
    ("host-setup", "/var/tmp/wt-host-setup", HOST_SETUP),
    (
        "host-defer-init",
        "/var/tmp/wt-host-defer-init",
        HOST_DEFER_INIT,
    ),
    (
        "host-cloud-config",
        "/var/tmp/wt-host-cloud-config",
        HOST_CLOUD_CONFIG,
    ),
    (
        "host-cloud-final",
        "/var/tmp/wt-host-cloud-final",
        HOST_CLOUD_FINAL,
    ),
    (
        "host-setup-service",
        "/var/tmp/wt-host-setup-service",
        HOST_SETUP_SERVICE,
    ),
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    source_sha256: String,
    config_sha256: String,
    inputs: BTreeMap<String, String>,
    golden_sha256: String,
    tmux_sha256: String,
    packages: PackageVersions,
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
    let installed = resolve(&server.image.path).map_err(anyhow::Error::msg)?;
    if !installed.current && (installed.image.exists() || installed.manifest.exists()) {
        bail!("legacy image layout exists; run make nuke before installing this version");
    }
    let source = source_image(input, runner)?;
    let byobu = byobu_package(runner)?;
    match installed_image_state(
        installed.image.exists(),
        installed.manifest.exists(),
        || verify_installed_image(input, server_bytes, &installed.image, &installed.manifest),
    ) {
        InstalledImageState::Reusable => {
            println!(
                "Reusing verified retained golden image: {}",
                server.image.path.display()
            );
        }
        InstalledImageState::Missing => {
            build_image(runner, input, server, server_bytes, &source, &byobu)?;
        }
        InstalledImageState::Replace(reason) => {
            println!("Replacing the installed retained golden image: {reason}");
            println!("Existing worlds use independent disks and are unaffected.");
            build_image(runner, input, server, server_bytes, &source, &byobu)?;
        }
    }
    Ok(())
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
    build_image(runner, input, server, server_bytes, &source, &byobu)?;
    Ok(())
}

pub(crate) fn verify(
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    let installed = resolve(&server.image.path).map_err(anyhow::Error::msg)?;
    verify_installed_image(input, server_bytes, &installed.image, &installed.manifest)?;
    println!(
        "Verified retained golden image and provenance: {}",
        server.image.path.display()
    );
    Ok(())
}

fn source_image(input: &InstallInput, runner: &impl Runner) -> Result<PathBuf> {
    let path = Path::new("imgs").join(SOURCE_IMAGE_NAME);
    fs::create_dir_all("imgs").context("create imgs directory")?;
    if path.exists() {
        println!("Verifying cached Ubuntu source image...");
        require_sha(&path, input.source_sha256(), "source image")?;
        println!("Reusing verified Ubuntu source image: {}", path.display());
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
        println!("Reusing verified Byobu package: {}", path.display());
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
) -> Result<()> {
    println!("Building retained golden image from verified source inputs.");
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
        build_image_inner(&context, server_bytes, &build_dir)
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
    build_dir: &Path,
) -> Result<()> {
    let runner = context.runner;
    let input = context.input;
    let server = context.server;
    let recipe = ImageRecipe::new();
    let spec = BuildSpec {
        name: BUILD_NAME,
        recipe: RETAINED_IMAGE_BUILD,
    };
    let staged_paths = HOST_INPUTS
        .iter()
        .map(|(name, _, bytes)| {
            let path = build_dir.join(name);
            fs::write(&path, bytes).context("stage retained image input")?;
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()?;
    let extra_inputs = staged_paths
        .iter()
        .zip(HOST_INPUTS)
        .map(|(source, (_, guest_path, _))| StagedInput { source, guest_path })
        .collect::<Vec<_>>();
    let paths = run_kvm_build(context, build_dir, &spec, &extra_inputs)?;
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
    println!("Validated retained image packages: {package_summary}");

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
    println!("Compacting retained golden image...");
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

    println!("Hashing and publishing retained golden image...");
    let manifest = ImageManifest {
        source_sha256: input.source_sha256().to_ascii_lowercase(),
        config_sha256: image_config_sha(server_bytes, input),
        inputs: retained_input_hashes(&spec),
        golden_sha256: sha_file(&paths.prepared)?,
        tmux_sha256,
        packages,
    };
    let publication = stage_publication(runner, &paths.prepared, &server.image.path, &manifest)?;
    fs::remove_dir_all(&paths.dir).context("remove image build directory")?;
    publication.publish(runner)?;
    println!(
        "Published retained golden image: {}",
        server.image.path.display()
    );
    Ok(())
}

pub(crate) fn verify_installed_image(
    input: &InstallInput,
    server_bytes: &[u8],
    image_path: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let recipe = ImageRecipe::new();
    require_named_file(image_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read image manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse image manifest {}", manifest_path.display()))?;
    if manifest.source_sha256 != input.source_sha256().to_ascii_lowercase()
        || manifest.config_sha256 != image_config_sha(server_bytes, input)
        || manifest.inputs
            != retained_input_hashes(&BuildSpec {
                name: BUILD_NAME,
                recipe: RETAINED_IMAGE_BUILD,
            })
        || !is_sha256(&manifest.tmux_sha256)
    {
        bail!("provenance does not match the current source or install input");
    }
    recipe
        .validate_package_versions(&manifest.packages)
        .context("installed image package provenance differs")?;
    require_sha(
        image_path,
        &manifest.golden_sha256,
        "installed golden image",
    )
}

fn retained_input_hashes(spec: &BuildSpec<'_>) -> BTreeMap<String, String> {
    let inputs = HOST_INPUTS
        .iter()
        .map(|(_, guest_path, bytes)| (*guest_path, *bytes))
        .collect::<Vec<_>>();
    staged_input_hashes(spec, &inputs)
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
