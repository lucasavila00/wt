mod recipe;

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
use std::time::{Duration, Instant};
use wt_command::cmd;
use wt_libvirt::LIBVIRT_URI;
use wt_provider::PackageVersions;
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

pub(crate) fn ensure(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    let manifest_path = manifest_path(&server.image.installed_path);
    match (server.image.installed_path.exists(), manifest_path.exists()) {
        (true, true) => {
            println!("Verifying installed golden image and provenance...");
            verify_installed_image(input, server, server_bytes, &manifest_path)?;
            println!("Reusing verified golden image.");
            return Ok(());
        }
        (false, false) => {}
        _ => bail!("image drift: image and manifest must either both exist or both be absent"),
    }

    let source = source_image(input, runner)?;
    build_image(runner, input, server, server_bytes, &source, &manifest_path)
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
) -> Result<()> {
    refuse_active_worlds(runner)?;
    let source = source_image(input, runner)?;
    let manifest = manifest_path(&server.image.installed_path);
    build_image(runner, input, server, server_bytes, &source, &manifest)
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

fn build_image(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
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
    manifest_path: &Path,
    disk: &Path,
    seed: &Path,
    user_data: &Path,
    meta_data: &Path,
    console: &Path,
    prepared: &Path,
) -> Result<()> {
    let recipe = ImageRecipe::new();
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
    let marker = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/var/lib/wt-image-ready",),
        "verify image readiness marker",
    )?;
    if marker.trim() != "ready" {
        bail!("image build finished without the expected readiness marker");
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
            "--run-command",
            CLEAR_MACHINE_ID,
        ),
        "clear golden image machine identity",
    )?;
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

struct ConsoleLog {
    file: fs::File,
    pending_line: Vec<u8>,
}

impl ConsoleLog {
    fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            file: fs::File::open(path).context("open image build console log")?,
            pending_line: Vec::new(),
        })
    }

    fn drain(&mut self) -> Result<Vec<String>> {
        let mut bytes = Vec::new();
        self.file
            .read_to_end(&mut bytes)
            .context("read image build console log")?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(extract_phase_markers(&mut self.pending_line, &bytes))
    }
}

fn extract_phase_markers(pending_line: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    const PREFIX: &str = "WT_IMAGE_PHASE=";

    pending_line.extend_from_slice(bytes);
    let mut phases = Vec::new();
    let mut consumed = 0;
    for (index, byte) in pending_line.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let line = String::from_utf8_lossy(&pending_line[consumed..index]);
        if let Some((_, phase)) = line.split_once(PREFIX) {
            phases.push(phase.trim_end_matches('\r').to_owned());
        }
        consumed = index + 1;
    }
    pending_line.drain(..consumed);
    phases
}

fn progress_message(phase: &str, elapsed: Duration) -> String {
    format!("Image build: {phase} (elapsed={}s)", elapsed.as_secs())
}

fn drain_console(console: &mut ConsoleLog, started: Instant) -> Result<Option<String>> {
    let phases = console.drain()?;
    let mut last_phase = None;
    for phase in phases {
        println!("{}", progress_message(&phase, started.elapsed()));
        last_phase = Some(phase);
    }
    Ok(last_phase)
}

fn wait_for_shutdown(runner: &impl Runner, console: &mut ConsoleLog) -> Result<()> {
    let started = Instant::now();
    let deadline = Instant::now() + IMAGE_BUILD_TIMEOUT;
    let mut next_state_check = Instant::now();
    let mut next_heartbeat = Instant::now() + Duration::from_secs(60);
    let mut phase = String::from("starting cloud-init");
    loop {
        if let Some(next_phase) = drain_console(console, started)? {
            phase = next_phase;
            next_heartbeat = Instant::now() + Duration::from_secs(60);
        }

        let now = Instant::now();
        if now >= next_state_check {
            let state = runner.text(
                cmd!("virsh", "-c", LIBVIRT_URI, "domstate", BUILD_NAME),
                "read image build domain state",
            )?;
            if state.trim() == "shut off" {
                drain_console(console, started)?;
                println!("Guest powered off after {}s.", started.elapsed().as_secs());
                return Ok(());
            }
            next_state_check = now + Duration::from_secs(3);
        }

        if now >= next_heartbeat {
            println!("{}", progress_message(&phase, started.elapsed()));
            next_heartbeat = now + Duration::from_secs(60);
        }
        if now >= deadline {
            bail!("timed out waiting for KVM image build guest");
        }
        thread::sleep(Duration::from_millis(250));
    }
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
    let _ = fs::remove_dir_all(build_dir);
}

fn publish_image(
    runner: &impl Runner,
    server: &ServerConfig,
    prepared: &Path,
    manifest_path: &Path,
    manifest: &ImageManifest,
) -> Result<()> {
    let image_temporary = sibling_temporary(&server.image.installed_path)?;
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
    sudo_move(runner, &image_temporary, &server.image.installed_path)?;
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
    require_named_file(&server.image.installed_path, "libvirt-qemu", "kvm", 0o644)?;
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
        &server.image.installed_path,
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
mod tests {
    use super::*;

    #[test]
    fn manifest_path_is_next_to_image() {
        assert_eq!(
            manifest_path(Path::new("/var/lib/wt/golden.qcow2")),
            Path::new("/var/lib/wt/golden.qcow2.manifest.json")
        );
    }

    #[test]
    fn sha_validation_detects_drift() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("image");
        fs::write(&file, b"expected").unwrap();
        let expected = sha_bytes(b"expected");
        require_sha(&file, &expected, "test image").unwrap();
        fs::write(&file, b"different").unwrap();
        assert!(require_sha(&file, &expected, "test image").is_err());
    }

    #[test]
    fn image_manifest_records_structured_package_versions() {
        let manifest = ImageManifest {
            version: IMAGE_MANIFEST_VERSION,
            recipe_version: recipe::RECIPE_VERSION,
            source_sha256: "source".to_owned(),
            config_sha256: "config".to_owned(),
            golden_sha256: "golden".to_owned(),
            packages: [("tmux".to_owned(), "3.4-1".to_owned())].into(),
            devcontainer_cli: wt_provider::DEVCONTAINER_CLI_VERSION.to_owned(),
        };

        let json = serde_json::to_value(manifest).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["recipe_version"], 1);
        assert_eq!(json["packages"]["tmux"], "3.4-1");
    }

    #[test]
    fn image_recipe_clears_reusable_machine_identity() {
        assert_eq!(
            CLEAR_MACHINE_ID,
            "truncate -s 0 /etc/machine-id && ln -sfn /etc/machine-id /var/lib/dbus/machine-id"
        );
    }

    #[test]
    fn phase_markers_are_extracted_across_partial_writes() {
        let mut pending = Vec::new();
        assert!(extract_phase_markers(&mut pending, b"booting\nWT_IMAGE_PH").is_empty());
        assert_eq!(
            extract_phase_markers(
                &mut pending,
                b"ASE=installing packages\r\nordinary output\nWT_IMAGE_PHASE=validating"
            ),
            ["installing packages"]
        );
        assert_eq!(
            extract_phase_markers(&mut pending, b" services\n"),
            ["validating services"]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn console_log_reads_only_appended_phase_markers() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("console.log");
        fs::write(&path, b"first\nWT_IMAGE_PHASE=booting\n").unwrap();
        let mut console = ConsoleLog::open(&path).unwrap();

        let phases = console.drain().unwrap();
        assert_eq!(phases, ["booting"]);

        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"second")
            .unwrap();
        assert!(console.drain().unwrap().is_empty());
        assert!(console.drain().unwrap().is_empty());
    }

    #[test]
    fn console_reader_opens_the_replaced_log() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("console.log");
        fs::write(&path, b"old inode\n").unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"WT_IMAGE_PHASE=installing packages\n").unwrap();

        let mut console = ConsoleLog::open(&path).unwrap();
        assert_eq!(console.drain().unwrap(), ["installing packages"]);
    }

    #[test]
    fn progress_output_is_phase_based() {
        let message = progress_message("installing packages", Duration::from_secs(60));
        insta::assert_snapshot!(message, @"Image build: installing packages (elapsed=60s)");
    }
}
