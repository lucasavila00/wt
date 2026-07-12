use crate::files::{
    require_named_file, require_root_file, sudo_install, sudo_install_owned, sudo_move,
};
use crate::host;
use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use wt_libvirt::{ServerConfig, LIBVIRT_URI};

const SOURCE_IMAGE_NAME: &str = "ubuntu-24.04-server-cloudimg-amd64.img";
const BUILD_NAME: &str = "wt-image-build";
const IMAGE_BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const IMAGE_RECIPE_VERSION: u32 = 1;
const DEVCONTAINER_CLI_VERSION: &str = "0.80.2";

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
        "curl",
        &[
            "-fL".into(),
            "--output".into(),
            temporary.as_os_str().to_owned(),
            config.image.source_url.clone().into(),
        ],
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
    prepared: &Path,
) -> Result<()> {
    println!("Preparing temporary KVM build disk...");
    runner.run(
        "qemu-img",
        &[
            "convert".into(),
            "-p".into(),
            "-O".into(),
            "qcow2".into(),
            source.as_os_str().to_owned(),
            disk.as_os_str().to_owned(),
        ],
        "copy source image",
    )?;
    runner.run(
        "qemu-img",
        &[
            "resize".into(),
            disk.as_os_str().to_owned(),
            format!("{}G", config.guest.disk_gib).into(),
        ],
        "resize image build disk",
    )?;
    fs::write(user_data, cloud_config()).context("write image cloud-init user-data")?;
    fs::write(
        meta_data,
        format!("instance-id: {BUILD_NAME}\nlocal-hostname: {BUILD_NAME}\n"),
    )
    .context("write image cloud-init meta-data")?;
    runner.run(
        "cloud-localds",
        &[
            seed.as_os_str().to_owned(),
            user_data.as_os_str().to_owned(),
            meta_data.as_os_str().to_owned(),
        ],
        "create image build seed",
    )?;
    runner.run(
        "virt-install",
        &[
            "--connect".into(),
            LIBVIRT_URI.into(),
            "--name".into(),
            BUILD_NAME.into(),
            "--memory".into(),
            config.guest.memory_mib.to_string().into(),
            "--vcpus".into(),
            config.guest.vcpus.to_string().into(),
            "--virt-type".into(),
            "kvm".into(),
            "--os-variant".into(),
            "ubuntu24.04".into(),
            "--import".into(),
            "--boot".into(),
            "uefi".into(),
            "--disk".into(),
            format!("path={},format=qcow2,bus=virtio", disk.display()).into(),
            "--disk".into(),
            format!("path={},device=cdrom", seed.display()).into(),
            "--network".into(),
            format!("network={},model=virtio", config.libvirt.network).into(),
            "--graphics".into(),
            "none".into(),
            "--noautoconsole".into(),
            "--wait".into(),
            "0".into(),
        ],
        "start KVM image build guest",
    )?;
    println!(
        "KVM build guest started. Waiting for cloud-init to install Docker, Compose, and guest agent."
    );
    println!("The guest will power off when ready. Timeout: 30 minutes.");
    wait_for_shutdown(runner)?;

    println!("Guest powered off. Verifying readiness and package versions...");
    let marker = runner.text(
        "sudo",
        &[
            "virt-cat".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
            "/var/lib/wt-image-ready".into(),
        ],
        "verify image readiness marker",
    )?;
    if marker.trim() != "ready" {
        bail!("image build finished without the expected readiness marker");
    }
    let packages = runner
        .text(
            "sudo",
            &[
                "virt-cat".into(),
                "-a".into(),
                disk.as_os_str().to_owned(),
                "/var/lib/wt-image-packages".into(),
            ],
            "read installed guest package versions",
        )?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if packages.len() != 7 {
        bail!("image package manifest must contain exactly seven packages");
    }
    println!("Verified packages: {}", packages.join(", "));

    undefine_build_domain(runner)?;
    println!("Sysprepping golden image...");
    runner.run(
        "sudo",
        &[
            "virt-sysprep".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
        ],
        "sysprep golden image",
    )?;
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    runner.run(
        "sudo",
        &[
            "chown".into(),
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()).into(),
            disk.as_os_str().to_owned(),
        ],
        "restore image build disk ownership",
    )?;
    println!("Compacting golden image...");
    runner.run(
        "qemu-img",
        &[
            "convert".into(),
            "-p".into(),
            "-O".into(),
            "qcow2".into(),
            disk.as_os_str().to_owned(),
            prepared.as_os_str().to_owned(),
        ],
        "compact golden image",
    )?;
    runner.run(
        "qemu-img",
        &["check".into(), prepared.as_os_str().to_owned()],
        "check golden image",
    )?;

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
package_update: true
packages:
  - docker.io
  - docker-compose-v2
  - qemu-guest-agent
  - git
  - openssh-server
  - nodejs
  - npm
runcmd:
  - systemctl enable --now docker.service qemu-guest-agent.service ssh.service
  - docker info
  - docker compose version
  - npm install --global @devcontainers/cli@0.80.2
  - devcontainer --version
  - dpkg-query -W -f='${Package}=${Version}\n' docker.io docker-compose-v2 qemu-guest-agent git openssh-server nodejs npm | sort > /var/lib/wt-image-packages
  - printf 'ready\n' > /var/lib/wt-image-ready
power_state:
  mode: poweroff
  timeout: 60
  condition: true
"#
}

fn wait_for_shutdown(runner: &impl Runner) -> Result<()> {
    let started = Instant::now();
    let deadline = Instant::now() + IMAGE_BUILD_TIMEOUT;
    let mut next_report = Duration::from_secs(15);
    loop {
        let state = runner.text(
            "virsh",
            &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
            "read image build domain state",
        )?;
        if state.trim() == "shut off" {
            return Ok(());
        }
        let elapsed = started.elapsed();
        if elapsed >= next_report {
            println!(
                "Still building guest: elapsed={}s, domain_state={}",
                elapsed.as_secs(),
                state.trim()
            );
            next_report = elapsed + Duration::from_secs(15);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for KVM image build guest");
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn domain_exists(runner: &impl Runner) -> Result<bool> {
    let output = runner.output("virsh", &args(["-c", LIBVIRT_URI, "dominfo", BUILD_NAME]))?;
    Ok(output.status.success())
}

fn undefine_build_domain(runner: &impl Runner) -> Result<()> {
    runner.run(
        "virsh",
        &args(["-c", LIBVIRT_URI, "undefine", BUILD_NAME, "--nvram"]),
        "undefine image build domain",
    )
}

fn cleanup_failed_build(runner: &impl Runner, build_dir: &Path) {
    if domain_exists(runner).unwrap_or(false) {
        let state = runner
            .text(
                "virsh",
                &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
                "read failed build domain state",
            )
            .unwrap_or_default();
        if state.trim() != "shut off" {
            let _ = runner.run(
                "virsh",
                &args(["-c", LIBVIRT_URI, "destroy", BUILD_NAME]),
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
        || manifest.packages.len() != 7
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
        "virsh",
        &args(["-c", LIBVIRT_URI, "list", "--state-running", "--name"]),
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
}
