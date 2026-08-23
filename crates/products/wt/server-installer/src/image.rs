mod builder;
mod cache;
mod console;
mod probe;
mod recipe;
mod timing;

#[cfg(test)]
use console::{extract_phase_markers, progress_message, ConsoleLog};

use self::builder::*;
use self::recipe::ImageRecipe;
use self::timing::{timed, TimedRunner};
use crate::host;
use crate::install_input::InstallInput;
use crate::server::binaries;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use recipe::PackageVersions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
#[cfg(test)]
use wt_server::image_generation::manifest_path;
use wt_server::image_generation::resolve;
use wt_server::ServerConfig;

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const DEVELOPMENT_TOOLS_CACHE_BUILD_NAME: &str = "wt-development-tools-cache-build";
const DEVELOPMENT_TOOLS_CACHE_NAME: &str = "wt-development-tools.qcow2";
const HOST_IMAGE_BUILD: &[u8] = include_bytes!("../../../../../assets/world/host/build-image.sh");
const CODEX_REQUIREMENTS: &[u8] =
    include_bytes!("../../../../../assets/world/host/codex-requirements.toml");
const HOST_SHELL: &[u8] = include_bytes!("../../../../../assets/world/host/shell.sh");
const HOST_PREPARE: &[u8] = include_bytes!("../../../../../assets/world/host/prepare.sh");
const HOST_INPUTS: &[(&str, &str, &[u8])] = &[
    ("host-shell", "/var/tmp/wt-host-shell", HOST_SHELL),
    ("host-prepare", "/var/tmp/wt-host-prepare", HOST_PREPARE),
];
const GUEST_BINARY_INPUTS: &[(&str, &str)] = &[("wt", "/var/tmp/wt-guest")];
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    commit: String,
    guest_identity: wt_guest::GuestIdentity,
    golden_sha256: String,
    packages: PackageVersions,
    development_tools: PackageVersions,
}

pub(crate) fn ensure(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
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
        || verify_installed_image(&installed.image, &installed.manifest),
    ) {
        InstalledImageState::Reusable => {
            println!(
                "Reusing verified host golden image: {}",
                server.image.path.display()
            );
        }
        InstalledImageState::Missing => {
            build_image(runner, input, server, &source, &byobu)?;
        }
        InstalledImageState::Replace(reason) => {
            println!("Replacing the installed host golden image: {reason}");
            build_image(runner, input, server, &source, &byobu)?;
        }
    }
    Ok(())
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
) -> Result<()> {
    let _lock = BuildLock::acquire()?;
    require_clean_build_state(runner, server)?;
    require_clean_publication_state(server)?;
    let source = source_image(input, runner)?;
    let byobu = byobu_package(runner)?;
    build_image(runner, input, server, &source, &byobu)?;
    Ok(())
}

pub(crate) fn verify(_input: &InstallInput, server: &ServerConfig) -> Result<()> {
    let installed = resolve(&server.image.path).map_err(anyhow::Error::msg)?;
    verify_installed_image(&installed.image, &installed.manifest)?;
    println!(
        "Verified host golden image and provenance: {}",
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
    source: &Path,
    byobu: &Path,
) -> Result<()> {
    println!("Building host golden image from verified source inputs.");
    let build_dir = server.libvirt.worlds_dir.join(BUILD_NAME);

    let cache_build_dir = server
        .libvirt
        .worlds_dir
        .join(DEVELOPMENT_TOOLS_CACHE_BUILD_NAME);
    if build_dir.exists()
        || cache_build_dir.exists()
        || domain_exists(runner, BUILD_NAME)?
        || domain_exists(runner, DEVELOPMENT_TOOLS_CACHE_BUILD_NAME)?
    {
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
        build_image_inner(&context, &build_dir)
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

fn build_image_inner<R: Runner>(context: &BuildContext<'_, R>, build_dir: &Path) -> Result<()> {
    let runner = context.runner;
    let input = context.input;
    let server = context.server;
    let recipe = ImageRecipe::new();
    let staged_paths = HOST_INPUTS
        .iter()
        .map(|(name, _, bytes)| {
            let path = build_dir.join(name);
            fs::write(&path, bytes).context("stage host image input")?;
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()?;
    let codex_requirements = build_dir.join("codex-requirements.toml");
    fs::write(&codex_requirements, CODEX_REQUIREMENTS).context("stage Codex requirements")?;
    let guest_binary_inputs = GUEST_BINARY_INPUTS
        .iter()
        .map(|(_, guest_path)| (binaries::guest_binary(), *guest_path))
        .collect::<Vec<_>>();
    let extra_inputs = staged_paths
        .iter()
        .zip(HOST_INPUTS)
        .map(|(source, (_, guest_path, _))| StagedInput { source, guest_path })
        .chain(std::iter::once(StagedInput {
            source: &codex_requirements,
            guest_path: "/var/tmp/wt-codex-requirements.toml",
        }))
        .chain(
            guest_binary_inputs
                .iter()
                .map(|(source, guest_path)| StagedInput { source, guest_path }),
        )
        .collect::<Vec<_>>();
    let source = cache::ensure(context)?;
    let spec = BuildSpec {
        name: BUILD_NAME,
        main_recipe: CACHED_IMAGE_BUILD,
        host_recipe: HOST_IMAGE_BUILD,
    };
    let cached_context = BuildContext {
        runner: context.runner,
        input: context.input,
        server: context.server,
        source: &source,
        byobu: context.byobu,
    };
    let paths = run_kvm_build(
        &cached_context,
        build_dir,
        &spec,
        &extra_inputs,
        BuildSource::ReusableImage,
    )?;
    let package_output = runner.timed_text(
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
    println!("Validated host image packages: {package_summary}");
    let development_tool_output = runner.timed_text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-image-development-tools",
        ),
        "read installed guest development tool versions",
    )?;
    let development_tools = recipe.parse_development_tool_versions(&development_tool_output)?;

    finalize_reusable_image(runner, &paths)?;
    let tmux_version = runner.timed_text(
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
    let finalized_package_output = runner.timed_text(
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
    let finalized_development_tool_output = runner.timed_text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-image-development-tools",
        ),
        "revalidate finalized guest development tool versions",
    )?;
    if recipe.parse_development_tool_versions(&finalized_development_tool_output)?
        != development_tools
    {
        bail!("finalized image development tool versions changed during sanitization");
    }
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    runner.timed_run(
        cmd!(
            "sudo",
            "chown",
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()),
            &paths.disk,
        ),
        "restore image build disk ownership",
    )?;
    runner.timed_run(
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
    runner.timed_run(
        cmd!("qemu-img", "check", &paths.prepared),
        "check golden image",
    )?;
    let manifest = ImageManifest {
        commit: wt_control_protocol::GIT_COMMIT_SHA.to_owned(),
        guest_identity: wt_guest::GUEST_IDENTITY,
        golden_sha256: timed("hash compacted host image", || sha_file(&paths.prepared))?,
        packages,
        development_tools,
    };
    let publication = timed("stage host image publication", || {
        stage_publication(runner, &paths.prepared, &server.image.path, &manifest)
    })?;
    if let Err(primary) = timed("probe host image boot and shared identity", || {
        probe::verify_publication(input, server, publication.image_path())
    }) {
        return match publication.discard(runner) {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(primary.context(format!(
                "discard staged publication after failed virtiofs probe: {cleanup:#}"
            ))),
        };
    }
    fs::remove_dir_all(&paths.dir).context("remove image build directory")?;
    publication.publish(runner)?;
    println!(
        "Published host golden image: {}",
        server.image.path.display()
    );
    Ok(())
}

pub(crate) fn verify_installed_image(image_path: &Path, manifest_path: &Path) -> Result<()> {
    let recipe = ImageRecipe::new();
    require_named_file(image_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read image manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse image manifest {}", manifest_path.display()))?;
    wt_guest::validate_guest_identity(manifest.guest_identity).map_err(anyhow::Error::msg)?;
    require_current_commit(&manifest.commit)?;
    recipe
        .validate_package_versions(&manifest.packages)
        .context("installed image package provenance differs")?;
    recipe
        .validate_development_tool_versions(&manifest.development_tools)
        .context("installed image development tool provenance differs")?;
    require_sha(
        image_path,
        &manifest.golden_sha256,
        "installed golden image",
    )
}

fn require_current_commit(commit: &str) -> Result<()> {
    if commit != wt_control_protocol::GIT_COMMIT_SHA {
        bail!("image commit does not match the current WT commit");
    }
    Ok(())
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

#[cfg(test)]
mod tests;
