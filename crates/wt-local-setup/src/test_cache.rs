use crate::files::{
    require_named_file, require_root_file, sudo_install, sudo_install_owned, sudo_move,
};
use crate::image;
use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use nix::unistd::{Uid, User};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use wt_libvirt::{SiteConfig, LIBVIRT_URI};

const BUILD_NAME: &str = "wt-integration-cache-build";
const BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const CACHE_RECIPE_VERSION: u32 = 1;
const FIXTURE_IMAGES_PATH: &str = "crates/wt-integration-tests/fixture-images.txt";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CacheManifest {
    version: u32,
    recipe_version: u32,
    base_golden_sha256: String,
    cache_sha256: String,
    images: Vec<String>,
    resolved_images: Vec<String>,
}

pub(crate) fn installed_path(golden: &Path) -> PathBuf {
    let stem = golden
        .file_stem()
        .expect("validated golden image has a file stem")
        .to_string_lossy();
    golden.with_file_name(format!("{stem}.integration-tests.qcow2"))
}

pub(crate) fn ensure(runner: &impl Runner, config: &SiteConfig, config_bytes: &[u8]) -> Result<()> {
    let cache = installed_path(&config.image.installed_path);
    let manifest = image::manifest_path(&cache);
    match (cache.exists(), manifest.exists()) {
        (true, true) => {
            println!("Verifying integration test image cache...");
            verify(config, config_bytes, &cache, &manifest)?;
            println!("Reusing verified integration test image cache.");
            Ok(())
        }
        (false, false) => build(runner, config, config_bytes, &cache, &manifest),
        _ => bail!(
            "integration test image cache drift: image and manifest must either both exist or both be absent"
        ),
    }
}

pub(crate) fn rebuild(
    runner: &impl Runner,
    config: &SiteConfig,
    config_bytes: &[u8],
) -> Result<()> {
    image::refuse_active_worlds(runner)?;
    let cache = installed_path(&config.image.installed_path);
    let manifest = image::manifest_path(&cache);
    build(runner, config, config_bytes, &cache, &manifest)
}

fn build(
    runner: &impl Runner,
    config: &SiteConfig,
    config_bytes: &[u8],
    installed: &Path,
    manifest_path: &Path,
) -> Result<()> {
    image::verify_installed_image(
        config,
        config_bytes,
        &image::manifest_path(&config.image.installed_path),
    )?;
    let build_dir = config.libvirt.worlds_dir.join(BUILD_NAME);
    if build_dir.exists() || domain_exists(runner)? {
        bail!("stale integration cache build state exists for {BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create integration cache build directory")?;
    let result = build_inner(runner, config, installed, manifest_path, &build_dir);
    if result.is_err() {
        cleanup_failed_build(runner, &build_dir);
    }
    result
}

fn build_inner(
    runner: &impl Runner,
    config: &SiteConfig,
    installed: &Path,
    manifest_path: &Path,
    build_dir: &Path,
) -> Result<()> {
    let disk = build_dir.join("disk.qcow2");
    let seed = build_dir.join("seed.img");
    let user_data = build_dir.join("user-data");
    let meta_data = build_dir.join("meta-data");
    let images = fixture_images()?;

    println!("Creating integration cache overlay...");
    runner.run(
        "qemu-img",
        &[
            "create".into(),
            "-q".into(),
            "-f".into(),
            "qcow2".into(),
            "-F".into(),
            "qcow2".into(),
            "-b".into(),
            config.image.installed_path.as_os_str().to_owned(),
            disk.as_os_str().to_owned(),
            format!("{}G", config.guest.disk_gib).into(),
        ],
        "create integration cache overlay",
    )?;
    fs::write(&user_data, cloud_config(&images))
        .context("write integration cache cloud-init user-data")?;
    fs::write(
        &meta_data,
        format!("instance-id: {BUILD_NAME}\nlocal-hostname: {BUILD_NAME}\n"),
    )
    .context("write integration cache cloud-init meta-data")?;
    runner.run(
        "cloud-localds",
        &[
            seed.as_os_str().to_owned(),
            user_data.as_os_str().to_owned(),
            meta_data.as_os_str().to_owned(),
        ],
        "create integration cache seed",
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
        "start integration cache build guest",
    )?;
    println!("Pulling integration fixture images in the cache-builder guest...");
    wait_for_shutdown(runner)?;

    let marker = virt_cat(
        runner,
        &disk,
        "/var/lib/wt-integration-cache-ready",
        "verify integration cache readiness marker",
    )?;
    if marker.trim() != "ready" {
        bail!("integration cache build finished without the expected readiness marker");
    }
    let resolved_images = virt_cat(
        runner,
        &disk,
        "/var/lib/wt-integration-cache-images",
        "read cached image digests",
    )?
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if resolved_images.len() != images.len()
        || resolved_images
            .iter()
            .any(|line| !line.contains("@sha256:"))
    {
        bail!("integration cache image digest manifest is incomplete");
    }

    undefine_build_domain(runner)?;
    println!("Sysprepping integration test image cache...");
    runner.run(
        "sudo",
        &[
            "virt-sysprep".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
        ],
        "sysprep integration test image cache",
    )?;
    let user = User::from_uid(Uid::effective())
        .context("look up site user")?
        .context("site user does not exist")?;
    runner.run(
        "sudo",
        &[
            "chown".into(),
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()).into(),
            disk.as_os_str().to_owned(),
        ],
        "restore integration cache disk ownership",
    )?;
    runner.run(
        "qemu-img",
        &["check".into(), disk.as_os_str().to_owned()],
        "check integration test image cache",
    )?;

    let manifest = CacheManifest {
        version: 1,
        recipe_version: CACHE_RECIPE_VERSION,
        base_golden_sha256: image::installed_golden_sha(&image::manifest_path(
            &config.image.installed_path,
        ))?,
        cache_sha256: image::sha_file(&disk)?,
        images,
        resolved_images,
    };
    publish(runner, installed, manifest_path, &disk, &manifest)?;
    fs::remove_dir_all(build_dir).context("remove integration cache build directory")?;
    Ok(())
}

fn verify(
    config: &SiteConfig,
    config_bytes: &[u8],
    installed: &Path,
    manifest_path: &Path,
) -> Result<()> {
    image::verify_installed_image(
        config,
        config_bytes,
        &image::manifest_path(&config.image.installed_path),
    )?;
    require_named_file(installed, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: CacheManifest = serde_json::from_slice(
        &fs::read(manifest_path)
            .with_context(|| format!("read cache manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse cache manifest {}", manifest_path.display()))?;
    let base_sha =
        image::installed_golden_sha(&image::manifest_path(&config.image.installed_path))?;
    if !manifest_matches(&manifest, &base_sha, &fixture_images()?) {
        bail!("integration test image cache provenance differs from the installed golden image or fixture");
    }
    image::require_sha(
        installed,
        &manifest.cache_sha256,
        "integration test image cache",
    )
}

fn publish(
    runner: &impl Runner,
    installed: &Path,
    manifest_path: &Path,
    disk: &Path,
    manifest: &CacheManifest,
) -> Result<()> {
    let image_temporary = image::sibling_temporary(installed)?;
    let manifest_temporary = image::sibling_temporary(manifest_path)?;
    if image_temporary.exists() || manifest_temporary.exists() {
        bail!("stale temporary integration cache state exists");
    }
    let local_manifest = disk.with_extension("manifest.json");
    fs::write(&local_manifest, serde_json::to_vec_pretty(manifest)?)
        .context("write integration cache manifest")?;
    sudo_install_owned(runner, disk, &image_temporary, "libvirt-qemu", "kvm", 0o644)?;
    sudo_install(runner, &local_manifest, &manifest_temporary, 0o644)?;
    sudo_move(runner, &image_temporary, installed)?;
    sudo_move(runner, &manifest_temporary, manifest_path)?;
    Ok(())
}

fn cloud_config(images: &[String]) -> String {
    let pulls = images
        .iter()
        .map(|image| format!("  - {}", json_command(&["docker", "pull", image])))
        .collect::<Vec<_>>()
        .join("\n");
    let inspect = format!(
        "docker image inspect --format '{{{{join .RepoDigests \",\"}}}}' {} > /var/lib/wt-integration-cache-images",
        images.join(" ")
    );
    format!(
        "#cloud-config\nruncmd:\n  - {}\n{pulls}\n  - {}\n  - {}\npower_state:\n  mode: poweroff\n  timeout: 60\n  condition: true\n",
        json_command(&["systemctl", "start", "docker.service"]),
        json_command(&["sh", "-c", &inspect]),
        json_command(&["sh", "-c", "printf 'ready\\n' > /var/lib/wt-integration-cache-ready"]),
    )
}

fn json_command(values: &[&str]) -> String {
    serde_json::to_string(values).expect("serialize fixed cloud-init command")
}

fn fixture_images() -> Result<Vec<String>> {
    let images = fs::read_to_string(FIXTURE_IMAGES_PATH)
        .with_context(|| format!("read {FIXTURE_IMAGES_PATH}"))?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if images.is_empty() {
        bail!("integration fixture image list is empty");
    }
    if images.iter().any(|image| {
        image.is_empty()
            || !image
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"./:_-@".contains(&byte))
    }) {
        bail!("integration fixture contains an invalid container image reference");
    }
    Ok(images)
}

fn manifest_matches(manifest: &CacheManifest, base_sha: &str, images: &[String]) -> bool {
    manifest.version == 1
        && manifest.recipe_version == CACHE_RECIPE_VERSION
        && manifest.base_golden_sha256 == base_sha
        && manifest.images == images
        && manifest.resolved_images.len() == manifest.images.len()
}

fn virt_cat(runner: &impl Runner, disk: &Path, path: &str, action: &str) -> Result<String> {
    runner.text(
        "sudo",
        &[
            "virt-cat".into(),
            "-a".into(),
            disk.as_os_str().to_owned(),
            path.into(),
        ],
        action,
    )
}

fn wait_for_shutdown(runner: &impl Runner) -> Result<()> {
    let started = Instant::now();
    let deadline = started + BUILD_TIMEOUT;
    let mut next_report = Duration::from_secs(15);
    loop {
        let state = runner.text(
            "virsh",
            &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
            "read integration cache build domain state",
        )?;
        if state.trim() == "shut off" {
            return Ok(());
        }
        let elapsed = started.elapsed();
        if elapsed >= next_report {
            println!(
                "Still caching fixture images: elapsed={}s, domain_state={}",
                elapsed.as_secs(),
                state.trim()
            );
            next_report = elapsed + Duration::from_secs(15);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for integration cache build guest");
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn domain_exists(runner: &impl Runner) -> Result<bool> {
    Ok(runner
        .output("virsh", &args(["-c", LIBVIRT_URI, "dominfo", BUILD_NAME]))?
        .status
        .success())
}

fn undefine_build_domain(runner: &impl Runner) -> Result<()> {
    runner.run(
        "virsh",
        &args(["-c", LIBVIRT_URI, "undefine", BUILD_NAME, "--nvram"]),
        "undefine integration cache build domain",
    )
}

fn cleanup_failed_build(runner: &impl Runner, build_dir: &Path) {
    if domain_exists(runner).unwrap_or(false) {
        let state = runner
            .text(
                "virsh",
                &args(["-c", LIBVIRT_URI, "domstate", BUILD_NAME]),
                "read failed integration cache build domain state",
            )
            .unwrap_or_default();
        if state.trim() != "shut off" {
            let _ = runner.run(
                "virsh",
                &args(["-c", LIBVIRT_URI, "destroy", BUILD_NAME]),
                "destroy failed integration cache build domain",
            );
        }
        let _ = undefine_build_domain(runner);
    }
    let _ = fs::remove_dir_all(build_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_next_to_golden_image() {
        assert_eq!(
            installed_path(Path::new("/var/lib/wt/images/wt.qcow2")),
            Path::new("/var/lib/wt/images/wt.integration-tests.qcow2")
        );
    }

    #[test]
    fn cloud_config_pulls_every_fixture_image() {
        let images = vec!["example/app:1".to_owned(), "example/db:2".to_owned()];
        let config = cloud_config(&images);
        assert!(config.contains("[\"docker\",\"pull\",\"example/app:1\"]"));
        assert!(config.contains("[\"docker\",\"pull\",\"example/db:2\"]"));
        assert!(config.contains("wt-integration-cache-ready"));
    }

    #[test]
    fn manifest_match_detects_base_and_fixture_drift() {
        let images = vec!["example/app:1".to_owned()];
        let manifest = CacheManifest {
            version: 1,
            recipe_version: CACHE_RECIPE_VERSION,
            base_golden_sha256: "base".to_owned(),
            cache_sha256: "cache".to_owned(),
            images: images.clone(),
            resolved_images: vec!["example/app@sha256:digest".to_owned()],
        };
        assert!(manifest_matches(&manifest, "base", &images));
        assert!(!manifest_matches(&manifest, "new-base", &images));
        assert!(!manifest_matches(
            &manifest,
            "base",
            &["example/app:2".to_owned()]
        ));
    }
}
