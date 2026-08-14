mod console;
mod recipe;

#[cfg(test)]
use console::{extract_phase_markers, progress_message};
use console::{wait_for_shutdown, ConsoleLog};

use self::recipe::ImageRecipe;
use crate::files::{
    require_named_file, require_root_file, sudo_install, sudo_install_owned, sudo_move,
};
use crate::host;
use crate::install_input::InstallInput;
use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
#[cfg(test)]
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wt_command::cmd;
use wt_devcontainer::PackageVersions;
use wt_libvirt::LIBVIRT_URI;
use wt_server::ServerConfig;

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const IMAGE_BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const IMAGE_MANIFEST_VERSION: u32 = 1;
const CLEAR_MACHINE_ID: &str =
    "truncate -s 0 /etc/machine-id && ln -sfn /etc/machine-id /var/lib/dbus/machine-id";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    version: u32,
    recipe_version: u32,
    source_sha256: String,
    config_sha256: String,
    golden_sha256: String,
    packages: PackageVersions,
    devcontainer_cli: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HostImageManifest {
    version: u32,
    recipe_version: u32,
    source_sha256: String,
    config_sha256: String,
    image_sha256: String,
}

const HOST_IMAGE_RECIPE_VERSION: u32 = 1;

pub(crate) fn ensure(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    let manifest_path = manifest_path(&server.image.devcontainer_path);
    match (
        server.image.devcontainer_path.exists(),
        manifest_path.exists(),
    ) {
        (true, true) => {
            println!("Verifying installed golden image and provenance...");
            verify_installed_image(input, server, server_bytes, &manifest_path)?;
            println!("Reusing verified golden image.");
        }
        (false, false) => {
            let source = source_image(input, runner)?;
            let byobu = byobu_package(runner)?;
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
        _ => bail!("image drift: image and manifest must either both exist or both be absent"),
    }
    let source = source_image(input, runner)?;
    ensure_host_image(runner, input, server, server_bytes, &source)
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    refuse_active_worlds(runner)?;
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
    build_host_image(runner, input, server, server_bytes, &source)
}

fn ensure_host_image(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
) -> Result<()> {
    let manifest_path = manifest_path(&server.image.host_path);
    match (server.image.host_path.exists(), manifest_path.exists()) {
        (true, true) => verify_host_image(input, server, server_bytes, &manifest_path),
        (false, false) => build_host_image(runner, input, server, server_bytes, source),
        _ => bail!("host image drift: image and manifest must both exist or both be absent"),
    }
}

fn build_host_image(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
) -> Result<()> {
    let build = server.libvirt.worlds_dir.join("wt-host-image-build.qcow2");
    if build.exists() {
        bail!("stale host image build exists: {}", build.display());
    }
    let result = (|| {
        runner.run(
            cmd!("qemu-img", "convert", "-p", "-O", "qcow2", source, &build),
            "copy host source image",
        )?;
        runner.run(
            cmd!(
                "qemu-img",
                "resize",
                &build,
                format!("{}G", input.image.build_disk_gib)
            ),
            "resize host image",
        )?;
        runner.run(
            cmd!(
                "sudo",
                "virt-customize",
                "-a",
                &build,
                "--network",
                "--install",
                "openssh-server,qemu-guest-agent",
                "--run-command",
                "systemctl enable qemu-guest-agent.service ssh.service"
            ),
            "install host image prerequisites",
        )?;
        runner.run(
            cmd!(
                "sudo",
                "virt-sysprep",
                "-a",
                &build,
                "--operations",
                "machine-id,ssh-hostkeys"
            ),
            "clear host image identity",
        )?;
        runner.run(
            cmd!("sudo", "chown", "wt:wt", &build),
            "own prepared host image",
        )?;
        runner.run(
            cmd!("sudo", "chmod", "0640", &build),
            "permit prepared host image reading",
        )?;
        runner.run(cmd!("qemu-img", "check", &build), "check host image")?;
        let manifest = HostImageManifest {
            version: IMAGE_MANIFEST_VERSION,
            recipe_version: HOST_IMAGE_RECIPE_VERSION,
            source_sha256: input.source_sha256().to_ascii_lowercase(),
            config_sha256: image_config_sha(server_bytes, input),
            image_sha256: sha_file(&build)?,
        };
        publish_host_image(runner, server, &build, &manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&build);
    }
    result
}

fn publish_host_image(
    runner: &impl Runner,
    server: &ServerConfig,
    prepared: &Path,
    manifest: &HostImageManifest,
) -> Result<()> {
    let manifest_path = manifest_path(&server.image.host_path);
    let image_temporary = sibling_temporary(&server.image.host_path)?;
    let manifest_temporary = sibling_temporary(&manifest_path)?;
    let local_manifest = prepared.with_extension("manifest.json");
    fs::write(&local_manifest, serde_json::to_vec_pretty(manifest)?)?;
    sudo_install_owned(
        runner,
        prepared,
        &image_temporary,
        "libvirt-qemu",
        "kvm",
        0o644,
    )?;
    sudo_install(runner, &local_manifest, &manifest_temporary, 0o644)?;
    sudo_move(runner, &image_temporary, &server.image.host_path)?;
    sudo_move(runner, &manifest_temporary, &manifest_path)?;
    fs::remove_file(local_manifest)?;
    fs::remove_file(prepared)?;
    Ok(())
}

fn verify_host_image(
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    require_named_file(&server.image.host_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: HostImageManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.version != IMAGE_MANIFEST_VERSION
        || manifest.recipe_version != HOST_IMAGE_RECIPE_VERSION
        || manifest.source_sha256 != input.source_sha256().to_ascii_lowercase()
        || manifest.config_sha256 != image_config_sha(server_bytes, input)
    {
        bail!("installed host image provenance differs from the current install input");
    }
    require_sha(
        &server.image.host_path,
        &manifest.image_sha256,
        "installed host image",
    )
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
    let disk = build_dir.join("disk.qcow2");
    let seed = build_dir.join("seed.img");
    let user_data = build_dir.join("user-data");
    let meta_data = build_dir.join("meta-data");
    let console = build_dir.join("console.log");
    let prepared = build_dir.join("golden.qcow2");

    if build_dir.exists() || domain_exists(runner)? {
        bail!("stale image build state exists for {BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create image build directory")?;
    let result = (|| {
        fs::set_permissions(&build_dir, fs::Permissions::from_mode(0o2770))
            .context("set image build directory permissions")?;
        host::ensure_qemu_search_acl(runner, &build_dir)?;
        build_image_inner(
            runner,
            input,
            server,
            server_bytes,
            source,
            byobu,
            manifest_path,
            &disk,
            &seed,
            &user_data,
            &meta_data,
            &console,
            &prepared,
        )
    })();
    if result.is_err() {
        cleanup_failed_build(runner, &build_dir);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_image_inner(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
    byobu: &Path,
    manifest_path: &Path,
    disk: &Path,
    seed: &Path,
    user_data: &Path,
    meta_data: &Path,
    console: &Path,
    prepared: &Path,
) -> Result<()> {
    let recipe = ImageRecipe::new();
    let build_dir = disk.parent().context("image build disk has no parent")?;
    println!("Preparing temporary KVM build disk...");
    runner.run(
        cmd!("qemu-img", "convert", "-p", "-O", "qcow2", source, disk),
        "copy source image",
    )?;
    runner.run(
        cmd!(
            "qemu-img",
            "resize",
            disk,
            format!("{}G", input.image.build_disk_gib),
        ),
        "resize image build disk",
    )?;
    runner.run(
        cmd!(
            "sudo",
            "virt-customize",
            "-a",
            disk,
            "--upload",
            format!("{}:/var/tmp/wt-byobu.deb", byobu.display()),
        ),
        "stage pinned Byobu package in image build disk",
    )?;
    fs::write(user_data, recipe.cloud_config()).context("write image cloud-init user-data")?;
    fs::write(
        meta_data,
        format!("instance-id: {BUILD_NAME}\nlocal-hostname: {BUILD_NAME}\n"),
    )
    .context("write image cloud-init meta-data")?;
    runner.run(
        cmd!("cloud-localds", seed, user_data, meta_data),
        "create image build seed",
    )?;
    fs::File::create_new(console).context("create image build console log")?;
    fs::set_permissions(console, fs::Permissions::from_mode(0o660))
        .context("set image build console log permissions")?;
    runner.run(
        cmd!(
            "virt-install",
            "--connect",
            LIBVIRT_URI,
            "--name",
            BUILD_NAME,
            "--memory",
            input.image.build_memory_mib.to_string(),
            "--vcpus",
            input.image.build_vcpus.to_string(),
            "--virt-type",
            "kvm",
            "--os-variant",
            "ubuntu24.04",
            "--import",
            "--boot",
            "uefi",
            "--disk",
            format!("path={},format=qcow2,bus=virtio", disk.display()),
            "--disk",
            format!("path={},device=cdrom", seed.display()),
            "--network",
            format!("network={},model=virtio", server.libvirt.network),
            "--serial",
            format!("file,path={}", console.display()),
            "--graphics",
            "none",
            "--noautoconsole",
            "--wait",
            "0",
        ),
        "start KVM image build guest",
    )?;
    runner.run(
        cmd!("sudo", "chmod", "0640", console),
        "permit image build console reading",
    )?;
    let mut console_log = ConsoleLog::open(console)?;
    println!(
        "KVM build guest started. Waiting for cloud-init to install Docker, Compose, and guest agent."
    );
    println!("The guest will power off when ready. Timeout: 30 minutes.");
    wait_for_shutdown(runner, &mut console_log)?;

    println!("Guest powered off. Verifying readiness and package versions...");
    let marker = read_build_file(
        runner,
        disk,
        console,
        "/var/lib/wt-image-ready",
        "verify image readiness marker",
    )?;
    if marker.trim() != "ready" {
        bail!("image build finished without the expected readiness marker");
    }
    let byobu_marker = read_build_file(
        runner,
        disk,
        console,
        "/var/lib/wt-byobu-ready",
        "verify pinned Byobu readiness marker",
    )?;
    if byobu_marker.trim() != "ready" {
        bail!("image build finished without the pinned Byobu readiness marker");
    }
    let tmux_marker = read_build_file(
        runner,
        disk,
        console,
        "/var/lib/wt-tmux-ready",
        "verify pinned tmux readiness marker",
    )?;
    if tmux_marker.trim() != "ready" {
        bail!("image build finished without the pinned tmux readiness marker");
    }
    let ghostty_terminfo_marker = read_build_file(
        runner,
        disk,
        console,
        "/var/lib/wt-ghostty-terminfo-ready",
        "verify Ghostty terminfo readiness marker",
    )?;
    if ghostty_terminfo_marker.trim() != "ready" {
        bail!("image build finished without the Ghostty terminfo readiness marker");
    }
    let package_output = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/var/lib/wt-image-packages",),
        "read installed guest package versions",
    )?;
    let packages = recipe.parse_package_versions(&package_output)?;
    let package_summary = packages
        .iter()
        .map(|(name, version)| format!("{name}={version}"))
        .collect::<Vec<_>>()
        .join(", ");
    println!("Verified packages: {package_summary}");

    undefine_build_domain(runner)?;
    runner.run(
        cmd!(
            "sudo",
            "virt-copy-out",
            "-a",
            disk,
            "/var/lib/wt-tmux",
            &build_dir
        ),
        "preserve pinned tmux across image sysprep",
    )?;
    println!("Sysprepping golden image...");
    runner.run(
        cmd!("sudo", "virt-sysprep", "-a", disk),
        "sysprep golden image",
    )?;
    runner.run(
        cmd!(
            "sudo",
            "virt-customize",
            "-a",
            disk,
            "--upload",
            format!("{}:/var/tmp/wt-tmux", build_dir.join("wt-tmux").display()),
            "--run-command",
            format!(
                "install -m 0755 /var/tmp/wt-tmux /usr/bin/tmux && /usr/bin/tmux -V > /var/lib/wt-tmux-version && printf '%s  %s\\n' {} /usr/share/terminfo/g/ghostty | sha256sum --check --strict && cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && TERM=ghostty tput colors > /dev/null && TERM=xterm-ghostty tput colors > /dev/null && rm -f /var/tmp/wt-tmux /var/lib/wt-tmux /var/lib/wt-byobu-ready /var/lib/wt-tmux-ready /var/lib/wt-ghostty-terminfo-ready && {CLEAR_MACHINE_ID}",
                recipe::GHOSTTY_TERMINFO_SHA256
            ),
        ),
        "restore tmux, verify Ghostty terminfo, and clear golden image machine identity",
    )?;
    let tmux_version = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/var/lib/wt-tmux-version",),
        "read pinned tmux version after image sysprep",
    )?;
    if tmux_version.trim() != format!("tmux {}", recipe::TMUX_VERSION) {
        bail!(
            "golden image has unexpected tmux version: {:?}",
            tmux_version.trim()
        );
    }
    let machine_id = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/etc/machine-id"),
        "verify empty golden image machine identity",
    )?;
    if !machine_id.is_empty() {
        bail!("golden image machine identity was not cleared");
    }
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    runner.run(
        cmd!(
            "sudo",
            "chown",
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()),
            disk,
        ),
        "restore image build disk ownership",
    )?;
    println!("Compacting golden image...");
    runner.run(
        cmd!("qemu-img", "convert", "-p", "-O", "qcow2", disk, prepared,),
        "compact golden image",
    )?;
    runner.run(cmd!("qemu-img", "check", prepared), "check golden image")?;

    println!("Hashing and publishing golden image...");
    let manifest = ImageManifest {
        version: IMAGE_MANIFEST_VERSION,
        recipe_version: recipe::RECIPE_VERSION,
        source_sha256: input.source_sha256().to_ascii_lowercase(),
        config_sha256: image_config_sha(server_bytes, input),
        golden_sha256: sha_file(prepared)?,
        packages,
        devcontainer_cli: recipe.devcontainer_cli_version().to_owned(),
    };
    publish_image(runner, server, prepared, manifest_path, &manifest)?;
    fs::remove_dir_all(server.libvirt.worlds_dir.join(BUILD_NAME))
        .context("remove image build directory")?;
    Ok(())
}

fn domain_exists(runner: &impl Runner) -> Result<bool> {
    let output = runner.output(cmd!("virsh", "-c", LIBVIRT_URI, "dominfo", BUILD_NAME))?;
    Ok(output.status.success())
}

fn undefine_build_domain(runner: &impl Runner) -> Result<()> {
    runner.run(
        cmd!(
            "virsh",
            "-c",
            LIBVIRT_URI,
            "undefine",
            BUILD_NAME,
            "--nvram",
        ),
        "undefine image build domain",
    )
}

fn read_build_file(
    runner: &impl Runner,
    disk: &Path,
    console: &Path,
    guest_path: &str,
    action: &str,
) -> Result<String> {
    match runner.text(cmd!("sudo", "virt-cat", "-a", disk, guest_path), action) {
        Ok(text) => Ok(text),
        Err(error) => {
            let log = fs::read_to_string(console).context("read failed image build console")?;
            let tail = log.lines().rev().take(500).collect::<Vec<_>>();
            bail!(
                "{error}\nImage build console tail:\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            )
        }
    }
}

fn cleanup_failed_build(runner: &impl Runner, build_dir: &Path) {
    if domain_exists(runner).unwrap_or(false) {
        let state = runner
            .text(
                cmd!("virsh", "-c", LIBVIRT_URI, "domstate", BUILD_NAME),
                "read failed build domain state",
            )
            .unwrap_or_default();
        if state.trim() != "shut off" {
            let _ = runner.run(
                cmd!("virsh", "-c", LIBVIRT_URI, "destroy", BUILD_NAME),
                "destroy failed build domain",
            );
        }
        let _ = undefine_build_domain(runner);
    }
    let console = build_dir.join("console.log");
    if console.exists() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let destination = build_dir.with_file_name(format!(
            "{BUILD_NAME}.failed-{suffix}-{}.console.log",
            std::process::id()
        ));
        match fs::copy(&console, &destination) {
            Ok(_) => eprintln!(
                "Preserved failed image build console: {}",
                destination.display()
            ),
            Err(error) => eprintln!("Could not preserve failed image build console: {error}"),
        }
    }
    let _ = fs::remove_dir_all(build_dir);
}

fn publish_image(
    runner: &impl Runner,
    server: &ServerConfig,
    prepared: &Path,
    manifest_path: &Path,
    manifest: &ImageManifest,
) -> Result<()> {
    let image_temporary = sibling_temporary(&server.image.devcontainer_path)?;
    let manifest_temporary = sibling_temporary(manifest_path)?;
    if image_temporary.exists() || manifest_temporary.exists() {
        bail!("stale temporary installed image state exists");
    }
    let local_manifest = prepared.with_extension("manifest.json");
    fs::write(&local_manifest, serde_json::to_vec_pretty(manifest)?)
        .context("write image manifest")?;
    sudo_install_owned(
        runner,
        prepared,
        &image_temporary,
        "libvirt-qemu",
        "kvm",
        0o644,
    )?;
    sudo_install(runner, &local_manifest, &manifest_temporary, 0o644)?;
    sudo_move(runner, &image_temporary, &server.image.devcontainer_path)?;
    sudo_move(runner, &manifest_temporary, manifest_path)?;
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
        || manifest.devcontainer_cli != recipe.devcontainer_cli_version()
    {
        bail!("installed image provenance differs from the current install input");
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

fn image_config_sha(server_bytes: &[u8], input: &InstallInput) -> String {
    let mut bytes = server_bytes.to_vec();
    bytes.extend_from_slice(
        format!(
            "\nimage_memory_mib={}\nimage_vcpus={}\nimage_disk_gib={}\n",
            input.image.build_memory_mib, input.image.build_vcpus, input.image.build_disk_gib
        )
        .as_bytes(),
    );
    sha_bytes(&bytes)
}

pub(crate) fn refuse_active_worlds(runner: &impl Runner) -> Result<()> {
    let names = runner.text(
        cmd!(
            "virsh",
            "-c",
            LIBVIRT_URI,
            "list",
            "--state-running",
            "--name",
        ),
        "list active libvirt domains",
    )?;
    let active = names
        .lines()
        .filter(|name| name.starts_with("wt-"))
        .collect::<Vec<_>>();
    if !active.is_empty() {
        bail!(
            "refusing image rebuild while wt domains are active: {}",
            active.join(", ")
        );
    }
    Ok(())
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

fn sha_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
