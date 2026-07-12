use crate::files::{
    require_named_file, require_root_file, sudo_install, sudo_install_owned, sudo_move,
};
use crate::host;
use crate::runner::Runner;
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use wt_command::cmd;
use wt_libvirt::{ServerConfig, LIBVIRT_URI};

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const IMAGE_BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const IMAGE_RECIPE_VERSION: u32 = 1;
const DEVCONTAINER_CLI_VERSION: &str = "0.80.2";
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
    packages: Vec<String>,
    devcontainer_cli: String,
}

pub(crate) fn ensure(
    runner: &impl Runner,
    config: &ServerConfig,
    config_bytes: &[u8],
) -> Result<()> {
    let manifest_path = manifest_path(&config.image.installed_path);
    match (config.image.installed_path.exists(), manifest_path.exists()) {
        (true, true) => {
            println!("Verifying installed golden image and provenance...");
            verify_installed_image(config, config_bytes, &manifest_path)?;
            println!("Reusing verified golden image.");
            return Ok(());
        }
        (false, false) => {}
        _ => bail!("image drift: image and manifest must either both exist or both be absent"),
    }

    let source = source_image(config, runner)?;
    build_image(runner, config, config_bytes, &source, &manifest_path)
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    config: &ServerConfig,
    config_bytes: &[u8],
) -> Result<()> {
    refuse_active_worlds(runner)?;
    let source = source_image(config, runner)?;
    let manifest = manifest_path(&config.image.installed_path);
    build_image(runner, config, config_bytes, &source, &manifest)
}

fn source_image(config: &ServerConfig, runner: &impl Runner) -> Result<PathBuf> {
    let path = Path::new("imgs").join(SOURCE_IMAGE_NAME);
    fs::create_dir_all("imgs").context("create imgs directory")?;
    if path.exists() {
        println!("Verifying cached Ubuntu source image...");
        require_sha(&path, &config.image.source_sha256, "source image")?;
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
        cmd!(
            "curl",
            "-fL",
            "--output",
            &temporary,
            &config.image.source_url,
        ),
        "download pinned Ubuntu image",
    )?;
    if let Err(error) = require_sha(&temporary, &config.image.source_sha256, "downloaded image") {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).context("publish source image")?;
    Ok(path)
}

fn build_image(
    runner: &impl Runner,
    config: &ServerConfig,
    config_bytes: &[u8],
    source: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let build_dir = config.libvirt.worlds_dir.join(BUILD_NAME);
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
            config,
            config_bytes,
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
    config: &ServerConfig,
    config_bytes: &[u8],
    source: &Path,
    manifest_path: &Path,
    disk: &Path,
    seed: &Path,
    user_data: &Path,
    meta_data: &Path,
    console: &Path,
    prepared: &Path,
) -> Result<()> {
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
            format!("{}G", config.guest.disk_gib),
        ),
        "resize image build disk",
    )?;
    fs::write(user_data, cloud_config()).context("write image cloud-init user-data")?;
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
    // Keep the reader open before libvirt takes ownership of the serial log.
    let mut console_log = ConsoleLog::open(console)?;
    runner.run(
        cmd!(
            "virt-install",
            "--connect",
            LIBVIRT_URI,
            "--name",
            BUILD_NAME,
            "--memory",
            config.guest.memory_mib.to_string(),
            "--vcpus",
            config.guest.vcpus.to_string(),
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
            format!("network={},model=virtio", config.libvirt.network),
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
    let packages = runner
        .text(
            cmd!("sudo", "virt-cat", "-a", disk, "/var/lib/wt-image-packages",),
            "read installed guest package versions",
        )?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if packages.len() != 8 {
        bail!("image package manifest must contain exactly eight packages");
    }
    println!("Verified packages: {}", packages.join(", "));

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
        version: 1,
        recipe_version: IMAGE_RECIPE_VERSION,
        source_sha256: config.image.source_sha256.to_ascii_lowercase(),
        config_sha256: sha_bytes(config_bytes),
        golden_sha256: sha_file(prepared)?,
        packages,
        devcontainer_cli: DEVCONTAINER_CLI_VERSION.to_owned(),
    };
    publish_image(runner, config, prepared, manifest_path, &manifest)?;
    fs::remove_dir_all(config.libvirt.worlds_dir.join(BUILD_NAME))
        .context("remove image build directory")?;
    Ok(())
}

fn cloud_config() -> &'static str {
    r#"#cloud-config
output:
  all: '| tee -a /var/log/cloud-init-output.log'
bootcmd:
  - echo 'WT_IMAGE_PHASE=updating package indexes and installing guest packages' > /dev/ttyS0
package_update: true
packages:
  - docker.io
  - docker-compose-v2
  - qemu-guest-agent
  - git
  - openssh-server
  - nodejs
  - npm
  - tmux
runcmd:
  - echo 'WT_IMAGE_PHASE=validating guest services' > /dev/ttyS0
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker compose version
  - echo 'WT_IMAGE_PHASE=installing and validating Dev Container CLI' > /dev/ttyS0
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - echo 'WT_IMAGE_PHASE=recording installed package versions' > /dev/ttyS0
  - dpkg-query -W -f='${Package}=${Version}\n' docker.io docker-compose-v2 qemu-guest-agent git openssh-server nodejs npm tmux | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
  - echo 'WT_IMAGE_PHASE=build ready; requesting shutdown' > /dev/ttyS0
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"#
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

    fn drain(&mut self, output: &mut impl Write) -> Result<(bool, Vec<String>)> {
        let mut bytes = Vec::new();
        self.file
            .read_to_end(&mut bytes)
            .context("read image build console log")?;
        if bytes.is_empty() {
            return Ok((false, Vec::new()));
        }

        output
            .write_all(&bytes)
            .context("forward image build console output")?;
        output.flush().context("flush image build console output")?;
        let phases = extract_phase_markers(&mut self.pending_line, &bytes);
        Ok((true, phases))
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

fn drain_console(console: &mut ConsoleLog, started: Instant) -> Result<bool> {
    let (had_output, phases) = console.drain(&mut std::io::stdout().lock())?;
    for phase in phases {
        println!(
            "Image build phase: {phase} (elapsed={}s)",
            started.elapsed().as_secs()
        );
    }
    Ok(had_output)
}

fn wait_for_shutdown(runner: &impl Runner, console: &mut ConsoleLog) -> Result<()> {
    let started = Instant::now();
    let deadline = Instant::now() + IMAGE_BUILD_TIMEOUT;
    let mut last_console_output = Instant::now();
    let mut next_state_check = Instant::now();
    let mut state = String::from("starting");
    loop {
        if drain_console(console, started)? {
            last_console_output = Instant::now();
        }

        let now = Instant::now();
        if now >= next_state_check {
            state = runner.text(
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

        if last_console_output.elapsed() >= Duration::from_secs(15) {
            println!(
                "Still building guest: no console output for {}s, elapsed={}s, domain_state={}",
                last_console_output.elapsed().as_secs(),
                started.elapsed().as_secs(),
                state.trim()
            );
            last_console_output = Instant::now();
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
    config: &ServerConfig,
    prepared: &Path,
    manifest_path: &Path,
    manifest: &ImageManifest,
) -> Result<()> {
    let image_temporary = sibling_temporary(&config.image.installed_path)?;
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
    sudo_move(runner, &image_temporary, &config.image.installed_path)?;
    sudo_move(runner, &manifest_temporary, manifest_path)?;
    Ok(())
}

pub(crate) fn verify_installed_image(
    config: &ServerConfig,
    config_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    require_named_file(&config.image.installed_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read image manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse image manifest {}", manifest_path.display()))?;
    if manifest.version != 1
        || manifest.recipe_version != IMAGE_RECIPE_VERSION
        || manifest.source_sha256 != config.image.source_sha256.to_ascii_lowercase()
        || manifest.config_sha256 != sha_bytes(config_bytes)
        || manifest.packages.len() != 8
        || manifest.devcontainer_cli != DEVCONTAINER_CLI_VERSION
    {
        bail!("installed image provenance differs from requested config");
    }
    require_sha(
        &config.image.installed_path,
        &manifest.golden_sha256,
        "installed golden image",
    )
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
    fn image_recipe_installs_and_records_tmux() {
        let config = cloud_config();
        assert!(config.contains("  - tmux\n"));
        assert!(config.contains("nodejs npm tmux | sort > /var/lib/wt-image-packages"));
    }

    #[test]
    fn image_recipe_clears_reusable_machine_identity() {
        assert_eq!(
            CLEAR_MACHINE_ID,
            "truncate -s 0 /etc/machine-id && ln -sfn /etc/machine-id /var/lib/dbus/machine-id"
        );
    }

    #[test]
    fn image_recipe_reports_build_phases() {
        let config = cloud_config();
        for phase in [
            "updating package indexes and installing guest packages",
            "validating guest services",
            "installing and validating Dev Container CLI",
            "recording installed package versions",
            "build ready; requesting shutdown",
        ] {
            assert!(config.contains(&format!("WT_IMAGE_PHASE={phase}")));
        }
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
    fn console_log_forwards_only_appended_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("console.log");
        fs::write(&path, b"first\nWT_IMAGE_PHASE=booting\n").unwrap();
        let mut console = ConsoleLog::open(&path).unwrap();
        let mut output = Vec::new();

        let (had_output, phases) = console.drain(&mut output).unwrap();
        assert!(had_output);
        assert_eq!(output, b"first\nWT_IMAGE_PHASE=booting\n");
        assert_eq!(phases, ["booting"]);

        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"second")
            .unwrap();
        let (had_output, phases) = console.drain(&mut output).unwrap();
        assert!(had_output);
        assert!(phases.is_empty());
        assert_eq!(output, b"first\nWT_IMAGE_PHASE=booting\nsecond");

        let (had_output, phases) = console.drain(&mut output).unwrap();
        assert!(!had_output);
        assert!(phases.is_empty());
    }
}
