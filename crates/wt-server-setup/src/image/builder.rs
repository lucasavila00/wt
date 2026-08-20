use super::console::{wait_for_shutdown, ConsoleLog};
use super::recipe::ImageRecipe;
use super::{host_image, manifest_path, recipe, sibling_temporary, BUILD_NAME};
use crate::install_input::InstallInput;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use wt_command::cmd;
use wt_libvirt::LIBVIRT_URI;
use wt_server::ServerConfig;
use wt_setup_core::{sudo_install, sudo_install_owned, sudo_move, Runner};

const INSTALL_PACKAGES: &[u8] =
    include_bytes!("../../../../assets/world/shared/install-packages.sh");
const INSTALL_TERMINAL: &[u8] =
    include_bytes!("../../../../assets/world/shared/install-terminal.sh");
const SHARED_IMAGE_BUILD: &[u8] = include_bytes!("../../../../assets/world/shared/build-image.sh");
const FINALIZE_IMAGE: &[u8] = include_bytes!("../../../../assets/world/shared/finalize-image.sh");
const TMUX_CONFIG: &[u8] = include_bytes!("../../../../assets/world/shared/tmux.conf");
const BYOBU_COLOR: &[u8] = include_bytes!("../../../../assets/world/shared/byobu-color");
const BUILD_LOCK_PATH: &str = "/run/wt-image-build/lock";

pub(super) struct BuildSpec<'a> {
    pub(super) name: &'a str,
    pub(super) kind: &'a str,
    pub(super) recipe_version: u32,
    pub(super) recipe: &'a [u8],
}

pub(super) struct BuildPaths {
    pub(super) dir: PathBuf,
    pub(super) disk: PathBuf,
    pub(super) console: PathBuf,
    pub(super) prepared: PathBuf,
}

pub(super) struct StagedInput<'a> {
    pub(super) source: &'a Path,
    pub(super) guest_path: &'a str,
}

pub(super) struct BuildContext<'a, R: Runner> {
    pub(super) runner: &'a R,
    pub(super) input: &'a InstallInput,
    pub(super) server: &'a ServerConfig,
    pub(super) source: &'a Path,
    pub(super) byobu: &'a Path,
}

pub(super) struct BuildLock {
    path: PathBuf,
}

impl BuildLock {
    pub(super) fn acquire() -> Result<Self> {
        Self::acquire_path(Path::new(BUILD_LOCK_PATH))
    }

    #[cfg(test)]
    pub(super) fn acquire_at(path: &Path) -> Result<Self> {
        Self::acquire_path(path)
    }

    fn acquire_path(path: &Path) -> Result<Self> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "acquire exclusive image build lock {}; remove stale lock only after confirming no image build is active",
                    path.display()
                )
            })?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            eprintln!(
                "Could not remove image build lock {}: {error}",
                self.path.display()
            );
        }
    }
}

pub(super) fn run_kvm_build<R: Runner>(
    context: &BuildContext<'_, R>,
    build_dir: &Path,
    spec: &BuildSpec<'_>,
    extra_inputs: &[StagedInput<'_>],
) -> Result<BuildPaths> {
    let runner = context.runner;
    let input = context.input;
    let server = context.server;
    let paths = BuildPaths {
        dir: build_dir.to_path_buf(),
        disk: build_dir.join("disk.qcow2"),
        console: build_dir.join("console.log"),
        prepared: build_dir.join("golden.qcow2"),
    };
    let seed = build_dir.join("seed.img");
    let user_data = build_dir.join("user-data");
    let meta_data = build_dir.join("meta-data");
    let environment = build_dir.join("build.env");
    let install_packages = build_dir.join("install-packages.sh");
    let install_terminal = build_dir.join("install-terminal.sh");
    let shared_recipe = build_dir.join("shared-build-image.sh");
    let kind_recipe = build_dir.join("kind-build-image.sh");
    let tmux_config = build_dir.join("tmux.conf");
    let byobu_color = build_dir.join("byobu-color");

    println!("Preparing temporary {} KVM build disk...", spec.kind);
    runner.run(
        cmd!(
            "qemu-img",
            "convert",
            "-p",
            "-O",
            "qcow2",
            context.source,
            &paths.disk
        ),
        "copy source image",
    )?;
    runner.run(
        cmd!(
            "qemu-img",
            "resize",
            &paths.disk,
            format!("{}G", input.image.build_disk_gib),
        ),
        "resize image build disk",
    )?;

    fs::write(
        &environment,
        recipe::build_environment(
            spec.kind,
            spec.recipe_version,
            &sha_bytes(TMUX_CONFIG),
            &sha_bytes(BYOBU_COLOR),
        ),
    )
    .context("write image build environment")?;
    fs::write(&install_packages, INSTALL_PACKAGES).context("write package installer")?;
    fs::write(&install_terminal, INSTALL_TERMINAL).context("write terminal installer")?;
    fs::write(&shared_recipe, SHARED_IMAGE_BUILD).context("write shared image recipe")?;
    fs::write(&kind_recipe, spec.recipe).context("write kind image recipe")?;
    fs::write(&tmux_config, TMUX_CONFIG).context("write shared tmux configuration")?;
    fs::write(&byobu_color, BYOBU_COLOR).context("write shared Byobu color setting")?;

    let mut customize = Command::new("sudo");
    customize.arg("virt-customize").arg("-a").arg(&paths.disk);
    for (source, guest_path) in [
        (context.byobu, "/var/tmp/wt-byobu.deb"),
        (environment.as_path(), "/var/tmp/wt-image-build.env"),
        (
            install_packages.as_path(),
            "/var/tmp/wt-install-packages.sh",
        ),
        (
            install_terminal.as_path(),
            "/var/tmp/wt-install-terminal.sh",
        ),
        (shared_recipe.as_path(), "/var/tmp/wt-image-build.sh"),
        (kind_recipe.as_path(), "/var/tmp/wt-kind-image-build.sh"),
        (tmux_config.as_path(), "/var/tmp/wt-tmux.conf"),
        (byobu_color.as_path(), "/var/tmp/wt-byobu-color"),
    ] {
        customize
            .arg("--upload")
            .arg(format!("{}:{guest_path}", source.display()));
    }
    for input in extra_inputs {
        customize
            .arg("--upload")
            .arg(format!("{}:{}", input.source.display(), input.guest_path));
    }
    runner.run(customize, "stage image build inputs")?;

    fs::write(&user_data, ImageRecipe::new().cloud_config())
        .context("write image cloud-init user-data")?;
    fs::write(
        &meta_data,
        format!(
            "instance-id: {}\nlocal-hostname: {}\n",
            spec.name, spec.name
        ),
    )
    .context("write image cloud-init meta-data")?;
    runner.run(
        cmd!("cloud-localds", &seed, &user_data, &meta_data),
        "create image build seed",
    )?;
    fs::File::create_new(&paths.console).context("create image build console log")?;
    fs::set_permissions(&paths.console, fs::Permissions::from_mode(0o660))
        .context("set image build console log permissions")?;
    let mut console_log = start_kvm_build_guest(
        runner,
        input,
        server,
        spec.name,
        &paths.disk,
        &seed,
        &paths.console,
    )?;
    println!(
        "{} KVM build guest started; waiting for its recipe to finish and power off (30 minute timeout).",
        spec.kind
    );
    wait_for_shutdown(runner, &mut console_log, spec.name)?;
    undefine_build_domain(runner, spec.name)?;

    let marker = read_build_file(
        runner,
        &paths.disk,
        &paths.console,
        "/var/lib/wt-image-result",
        "verify image build result",
    )?;
    let marker_metadata = runner.text(
        cmd!(
            "sudo",
            "virt-ls",
            "--long",
            "--recursive",
            "--uids",
            "-a",
            &paths.disk,
            "/var/lib"
        ),
        "verify image build result metadata",
    )?;
    validate_result_metadata(&marker_metadata)?;
    let expected = format!(
        "kind={}\nstatus=ready\nrecipe_version={}\nwt_uid=1001\nwt_gid=1001\n",
        spec.kind, spec.recipe_version
    );
    if marker != expected {
        bail!(
            "{} image build returned an unexpected result marker: {:?}",
            spec.kind,
            marker
        );
    }
    println!("Verified {} image build result.", spec.kind);
    Ok(paths)
}

pub(super) fn finalize_reusable_image(runner: &impl Runner, paths: &BuildPaths) -> Result<String> {
    runner.run(
        cmd!(
            "sudo",
            "virt-copy-out",
            "-a",
            &paths.disk,
            "/var/lib/wt-tmux",
            &paths.dir
        ),
        "preserve pinned tmux across image sysprep",
    )?;
    println!("Sysprepping and sanitizing reusable image...");
    runner.run(
        cmd!("sudo", "virt-sysprep", "-a", &paths.disk),
        "sysprep reusable image",
    )?;
    let finalizer = paths.dir.join("finalize-image.sh");
    fs::write(&finalizer, FINALIZE_IMAGE).context("write image finalizer")?;
    runner.run(
        cmd!(
            "sudo",
            "virt-customize",
            "-a",
            &paths.disk,
            "--upload",
            format!("{}:/var/tmp/wt-tmux", paths.dir.join("wt-tmux").display()),
            "--upload",
            format!(
                "{}:/var/tmp/wt-image-build.env",
                paths.dir.join("build.env").display()
            ),
            "--run",
            &finalizer
        ),
        "finalize reusable image",
    )?;
    runner.run(
        cmd!(
            "sudo",
            "virt-sysprep",
            "-a",
            &paths.disk,
            "--operations",
            "ssh-hostkeys"
        ),
        "clear reusable image SSH host keys",
    )?;
    let machine_id = runner.text(
        cmd!("sudo", "virt-cat", "-a", &paths.disk, "/etc/machine-id"),
        "verify empty reusable image machine identity",
    )?;
    if !machine_id.is_empty() {
        bail!("reusable image machine identity was not cleared");
    }
    for path in [
        "/var/lib/cloud/instance",
        "/var/lib/cloud/instances",
        "/var/lib/cloud/seed",
        "/etc/netplan/50-cloud-init.yaml",
    ] {
        let output = runner.output(cmd!("sudo", "virt-ls", "-a", &paths.disk, path))?;
        if output.status.success() {
            bail!("reusable image retained cloud-init state at {path}");
        }
    }
    let ssh_files = runner.text(
        cmd!("sudo", "virt-ls", "-a", &paths.disk, "/etc/ssh"),
        "inspect reusable image SSH state",
    )?;
    if ssh_files.lines().any(|name| name.starts_with("ssh_host_")) {
        bail!("reusable image retained SSH host keys");
    }
    let tmux_sha256 = runner
        .text(
            cmd!(
                "sudo",
                "virt-cat",
                "-a",
                &paths.disk,
                "/var/lib/wt-tmux-sha256"
            ),
            "read finalized tmux checksum",
        )?
        .trim()
        .to_owned();
    if tmux_sha256.len() != 64 || !tmux_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("finalized image recorded an invalid tmux checksum");
    }
    Ok(tmux_sha256)
}

pub(super) fn staged_input_hashes(
    spec: &BuildSpec<'_>,
    extra_inputs: &[(&str, &[u8])],
) -> BTreeMap<String, String> {
    let environment = recipe::build_environment(
        spec.kind,
        spec.recipe_version,
        &sha_bytes(TMUX_CONFIG),
        &sha_bytes(BYOBU_COLOR),
    );
    let mut inputs = BTreeMap::from([
        (
            "/var/tmp/wt-byobu.deb".to_owned(),
            recipe::BYOBU_SHA256.to_owned(),
        ),
        (
            "/var/tmp/wt-image-build.env".to_owned(),
            sha_bytes(environment.as_bytes()),
        ),
        (
            "/var/tmp/wt-install-packages.sh".to_owned(),
            sha_bytes(INSTALL_PACKAGES),
        ),
        (
            "/var/tmp/wt-install-terminal.sh".to_owned(),
            sha_bytes(INSTALL_TERMINAL),
        ),
        (
            "/var/tmp/wt-image-build.sh".to_owned(),
            sha_bytes(SHARED_IMAGE_BUILD),
        ),
        (
            "/var/tmp/wt-kind-image-build.sh".to_owned(),
            sha_bytes(spec.recipe),
        ),
        ("/var/tmp/wt-tmux.conf".to_owned(), sha_bytes(TMUX_CONFIG)),
        ("/var/tmp/wt-byobu-color".to_owned(), sha_bytes(BYOBU_COLOR)),
        (
            "offline:/wt-finalize-image.sh".to_owned(),
            sha_bytes(FINALIZE_IMAGE),
        ),
        (
            "nocloud:user-data".to_owned(),
            sha_bytes(ImageRecipe::new().cloud_config().as_bytes()),
        ),
        (
            "nocloud:meta-data".to_owned(),
            sha_bytes(
                format!(
                    "instance-id: {}\nlocal-hostname: {}\n",
                    spec.name, spec.name
                )
                .as_bytes(),
            ),
        ),
    ]);
    for (path, bytes) in extra_inputs {
        inputs.insert((*path).to_owned(), sha_bytes(bytes));
    }
    inputs
}

pub(super) fn validate_result_metadata(listing: &str) -> Result<()> {
    let fields = listing
        .lines()
        .find(|line| line.ends_with(" /var/lib/wt-image-result"))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    if fields.len() < 6
        || fields[0] != "-"
        || fields[1] != "0644"
        || fields[3] != "0"
        || fields[4] != "0"
    {
        bail!("image build result must be owned by root:root with mode 0644");
    }
    Ok(())
}

pub(super) fn attach_console_tail(error: anyhow::Error, build_dir: &Path) -> anyhow::Error {
    let console = build_dir.join("console.log");
    let Ok(log) = fs::read_to_string(&console) else {
        return error;
    };
    let tail = log.lines().rev().take(500).collect::<Vec<_>>();
    error.context(format!(
        "Image build console tail:\n{}",
        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
    ))
}

fn start_kvm_build_guest(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    name: &str,
    disk: &Path,
    seed: &Path,
    console: &Path,
) -> Result<ConsoleLog> {
    runner.run(
        cmd!(
            "virt-install",
            "--connect",
            LIBVIRT_URI,
            "--name",
            name,
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
    ConsoleLog::open(console)
}

pub(super) fn domain_exists(runner: &impl Runner, name: &str) -> Result<bool> {
    let names = runner.text(
        cmd!("virsh", "-c", LIBVIRT_URI, "list", "--all", "--name"),
        "list libvirt domains",
    )?;
    Ok(names.lines().any(|candidate| candidate == name))
}

pub(super) fn require_clean_build_state(runner: &impl Runner, server: &ServerConfig) -> Result<()> {
    for name in [BUILD_NAME, host_image::BUILD_NAME] {
        let directory = server.libvirt.worlds_dir.join(name);
        if directory.exists() || domain_exists(runner, name)? {
            bail!("stale image build state exists for {name}");
        }
    }
    Ok(())
}

pub(super) fn require_clean_publication_state(server: &ServerConfig) -> Result<()> {
    for image in [&server.image.devcontainer_path, &server.image.host_path] {
        let manifest = manifest_path(image);
        let image_temporary = sibling_temporary(image)?;
        let manifest_temporary = sibling_temporary(&manifest)?;
        if image_temporary.exists() || manifest_temporary.exists() {
            bail!(
                "image publication drift: remove abandoned temporary files {} and {} with make nuke",
                image_temporary.display(),
                manifest_temporary.display()
            );
        }
    }
    Ok(())
}

fn undefine_build_domain(runner: &impl Runner, name: &str) -> Result<()> {
    runner.run(
        cmd!("virsh", "-c", LIBVIRT_URI, "undefine", name, "--nvram",),
        "undefine image build domain",
    )
}

pub(super) fn read_build_file(
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

pub(super) fn cleanup_failed_build(
    runner: &impl Runner,
    build_dir: &Path,
    name: &str,
) -> Result<()> {
    let mut failures = Vec::new();

    let mut domain_removed = true;
    match domain_exists(runner, name) {
        Ok(true) => {
            match runner.text(
                cmd!("virsh", "-c", LIBVIRT_URI, "domstate", name),
                "read failed build domain state",
            ) {
                Ok(state) if state.trim() == "shut off" => {}
                Ok(_) => {
                    if let Err(error) = runner.run(
                        cmd!("virsh", "-c", LIBVIRT_URI, "destroy", name),
                        "destroy failed build domain",
                    ) {
                        failures.push(error.to_string());
                        domain_removed = false;
                    }
                }
                Err(error) => {
                    failures.push(error.to_string());
                    domain_removed = false;
                }
            }
            if domain_removed {
                if let Err(error) = undefine_build_domain(runner, name) {
                    failures.push(error.to_string());
                    domain_removed = false;
                }
            }
        }
        Ok(false) => {}
        Err(error) => {
            failures.push(error.to_string());
            domain_removed = false;
        }
    }
    let console = build_dir.join("console.log");
    if console.exists() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let destination = build_dir.with_file_name(format!(
            "{name}.failed-{suffix}-{}.console.log",
            std::process::id()
        ));
        match fs::copy(&console, &destination) {
            Ok(_) => eprintln!(
                "Preserved failed image build console: {}",
                destination.display()
            ),
            Err(error) => {
                failures.push(format!("preserve failed image build console: {error}"));
                domain_removed = false;
            }
        }
    }
    if domain_removed && build_dir.exists() {
        if let Err(error) = fs::remove_dir_all(build_dir) {
            failures.push(format!("remove failed image build directory: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(failures.join("; "))
    }
}

pub(super) struct PendingPublication {
    image_temporary: PathBuf,
    manifest_temporary: PathBuf,
    image_destination: PathBuf,
    manifest_destination: PathBuf,
}

impl PendingPublication {
    pub(super) fn publish(self, runner: &impl Runner) -> Result<()> {
        sudo_move(runner, &self.image_temporary, &self.image_destination)?;
        sudo_move(runner, &self.manifest_temporary, &self.manifest_destination)
    }
}

pub(super) fn stage_publication<T: Serialize>(
    runner: &impl Runner,
    prepared: &Path,
    image_destination: &Path,
    manifest_path: &Path,
    manifest: &T,
) -> Result<PendingPublication> {
    let image_temporary = sibling_temporary(image_destination)?;
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
    Ok(PendingPublication {
        image_temporary,
        manifest_temporary,
        image_destination: image_destination.to_path_buf(),
        manifest_destination: manifest_path.to_path_buf(),
    })
}

pub(super) fn image_config_sha(server_bytes: &[u8], input: &InstallInput) -> String {
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

pub(super) fn sha_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
