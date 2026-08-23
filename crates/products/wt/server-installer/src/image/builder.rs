mod contract;
mod provenance;

pub(super) use contract::validate_result_metadata;
pub(super) use provenance::{sha_bytes, stage_publication};

use super::timing::TimedRunner;
use contract::verify_guest_contract;

use super::console::{wait_for_shutdown, ConsoleLog};
use super::{recipe, sibling_temporary, BUILD_NAME};
use crate::install_input::InstallInput;
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use wt_installer_support::cmd;
use wt_installer_support::Runner;
use wt_libvirt_kvm::LIBVIRT_URI;
use wt_server::image_generation::{current_path, generations_path};
use wt_server::ServerConfig;

const INSTALL_PACKAGES: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-packages.sh");
pub(super) const INSTALL_DEVELOPMENT_TOOLS: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-development-tools.sh");
const INSTALL_TERMINAL: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-terminal.sh");
const INSTALL_CODEX: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-codex.sh");
const INSTALL_DIFFO: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-diffo.sh");
pub(super) const DEVELOPMENT_TOOLS_CACHE_BUILD: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/build-development-tools-cache.sh");
pub(super) const CACHED_IMAGE_BUILD: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/build-image-from-cache.sh");
const FINALIZE_IMAGE: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/finalize-image.sh");
pub(super) const FINALIZE_DEVELOPMENT_TOOLS_CACHE: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/finalize-development-tools-cache.sh");
const TMUX_CONFIG: &[u8] = include_bytes!("../../../../../../assets/world/shared/tmux.conf");
const BYOBU_COLOR: &[u8] = include_bytes!("../../../../../../assets/world/shared/byobu-color");
const CONFIGURE_ACCESS: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/configure-access.sh");
const CONFIGURE_GIT_AUTHOR: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/configure-git-author.sh");
const INSTALL_AGENT_TOOLS: &[u8] =
    include_bytes!("../../../../../../assets/world/shared/install-agent-tools.sh");
const MOUNT_CODEX: &[u8] = include_bytes!("../../../../../../assets/world/shared/mount-codex.sh");
const NETWORK_CONFIG: &[u8] = b"network:\n  version: 2\n  ethernets:\n    primary:\n      match:\n        name: \"en*\"\n      dhcp4: true\n      dhcp-identifier: mac\n";
const BUILD_LOCK_PATH: &str = "/run/wt-image-build/lock";
pub(super) const IMAGE_KIND: &str = "guest";

pub(super) struct BuildSpec<'a> {
    pub(super) name: &'a str,
    pub(super) main_recipe: &'a [u8],
    pub(super) guest_recipe: &'a [u8],
}

pub(super) struct BuildPaths {
    pub(super) dir: PathBuf,
    pub(super) disk: PathBuf,
    pub(super) console: PathBuf,
    pub(super) prepared: PathBuf,
}

#[derive(Clone, Copy)]
pub(super) enum BuildSource {
    CloudImage,
    ReusableImage,
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
    source: BuildSource,
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
    let network_config = build_dir.join("network.yaml");
    let environment = build_dir.join("build.env");
    let install_packages = build_dir.join("install-packages.sh");
    let install_development_tools = build_dir.join("install-development-tools.sh");
    let install_terminal = build_dir.join("install-terminal.sh");
    let install_codex = build_dir.join("install-codex.sh");
    let install_diffo = build_dir.join("install-diffo.sh");
    let shared_recipe = build_dir.join("shared-build-image.sh");
    let guest_recipe = build_dir.join("guest-build-image.sh");
    let tmux_config = build_dir.join("tmux.conf");
    let byobu_color = build_dir.join("byobu-color");
    let configure_access = build_dir.join("configure-access.sh");
    let configure_git_author = build_dir.join("configure-git-author.sh");
    let install_agent_tools = build_dir.join("install-agent-tools.sh");
    let mount_codex = build_dir.join("mount-codex.sh");

    match source {
        BuildSource::CloudImage => {
            runner.timed_run(
                cmd!(
                    "qemu-img",
                    "create",
                    "-q",
                    "-f",
                    "qcow2",
                    &paths.disk,
                    format!("{}G", input.image.build_disk_gib),
                ),
                "create image build disk",
            )?;
            runner.timed_run(
                cmd!(
                    "sudo",
                    "virt-resize",
                    "--expand",
                    "/dev/sda1",
                    context.source,
                    &paths.disk,
                ),
                "copy and expand source image",
            )?;
        }
        BuildSource::ReusableImage => {
            runner.timed_run(
                cmd!(
                    "cp",
                    "--reflink=auto",
                    "--sparse=always",
                    "--",
                    context.source,
                    &paths.disk
                ),
                "clone cached image build disk",
            )?;
        }
    }

    fs::write(
        &environment,
        recipe::BuildEnvironment {
            kind: IMAGE_KIND,
            node_version: recipe::node_version(),
            tmux_config_sha256: &sha_bytes(TMUX_CONFIG),
            byobu_color_sha256: &sha_bytes(BYOBU_COLOR),
            access_sha256: &sha_bytes(CONFIGURE_ACCESS),
            git_author_sha256: &sha_bytes(CONFIGURE_GIT_AUTHOR),
            agent_tools_sha256: &sha_bytes(INSTALL_AGENT_TOOLS),
            mount_codex_sha256: &sha_bytes(MOUNT_CODEX),
        }
        .render(),
    )
    .context("write image build environment")?;
    fs::write(&install_packages, INSTALL_PACKAGES).context("write package installer")?;
    fs::write(&install_development_tools, INSTALL_DEVELOPMENT_TOOLS)
        .context("write development tools installer")?;
    fs::write(&install_terminal, INSTALL_TERMINAL).context("write terminal installer")?;
    fs::write(&install_codex, INSTALL_CODEX).context("write Codex installer")?;
    fs::write(&install_diffo, INSTALL_DIFFO).context("write Diffo installer")?;
    fs::write(&shared_recipe, spec.main_recipe).context("write image recipe")?;
    fs::write(&guest_recipe, spec.guest_recipe).context("write guest image recipe")?;
    fs::write(&tmux_config, TMUX_CONFIG).context("write shared tmux configuration")?;
    fs::write(&byobu_color, BYOBU_COLOR).context("write shared Byobu color setting")?;
    fs::write(&configure_access, CONFIGURE_ACCESS).context("write shared guest access setup")?;
    fs::write(&configure_git_author, CONFIGURE_GIT_AUTHOR)
        .context("write shared guest Git author setup")?;
    fs::write(&install_agent_tools, INSTALL_AGENT_TOOLS)
        .context("write shared agent tool setup")?;
    fs::write(&mount_codex, MOUNT_CODEX).context("write Codex mount setup")?;
    fs::write(&network_config, NETWORK_CONFIG).context("write image network configuration")?;

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
            install_development_tools.as_path(),
            "/var/tmp/wt-install-development-tools.sh",
        ),
        (
            install_terminal.as_path(),
            "/var/tmp/wt-install-terminal.sh",
        ),
        (install_codex.as_path(), "/var/tmp/wt-install-codex.sh"),
        (install_diffo.as_path(), "/var/tmp/wt-install-diffo.sh"),
        (shared_recipe.as_path(), "/var/tmp/wt-image-build.sh"),
        (guest_recipe.as_path(), "/var/tmp/wt-guest-image-build.sh"),
        (tmux_config.as_path(), "/var/tmp/wt-tmux.conf"),
        (byobu_color.as_path(), "/var/tmp/wt-byobu-color"),
        (configure_access.as_path(), "/var/tmp/wt-guest-access"),
        (
            configure_git_author.as_path(),
            "/var/tmp/wt-guest-git-author",
        ),
        (
            install_agent_tools.as_path(),
            "/var/tmp/wt-guest-agent-tools",
        ),
        (mount_codex.as_path(), "/var/tmp/wt-guest-mount-codex"),
        (network_config.as_path(), "/etc/netplan/50-wt.yaml"),
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
    customize
        .arg("--delete")
        .arg("/etc/netplan/50-cloud-init.yaml")
        .arg("--mkdir")
        .arg("/etc/cloud")
        .arg("--touch")
        .arg("/etc/cloud/cloud-init.disabled")
        .arg("--firstboot-command")
        .arg("/bin/sh /var/tmp/wt-image-build.sh");
    runner.timed_run(customize, "stage image build inputs with libguestfs")?;
    fs::File::create_new(&paths.console).context("create image build console log")?;
    fs::set_permissions(&paths.console, fs::Permissions::from_mode(0o660))
        .context("set image build console log permissions")?;
    let mut console_log = start_kvm_build_guest(
        runner,
        input,
        server,
        spec.name,
        &paths.disk,
        &paths.console,
    )?;
    println!(
        "Building {} golden image in a temporary KVM guest (30-minute timeout)...",
        IMAGE_KIND
    );
    wait_for_shutdown(runner, &mut console_log, spec.name, "Host")?;
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
        "kind={}\nstatus=ready\nwt_uid={}\nwt_gid={}\n",
        IMAGE_KIND,
        wt_guest::GUEST_UID,
        wt_guest::GUEST_GID,
    );
    if marker != expected {
        bail!(
            "{} image build returned an unexpected result marker: {:?}",
            IMAGE_KIND,
            marker
        );
    }
    println!("Validated {IMAGE_KIND} image recipe output.");
    Ok(paths)
}

pub(super) fn finalize_reusable_image(runner: &impl Runner, paths: &BuildPaths) -> Result<()> {
    runner.timed_run(
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
    println!("Finalizing {IMAGE_KIND} golden image for reuse (sysprep and sanitization)...");
    runner.timed_run(
        cmd!(
            "sudo",
            "virt-sysprep",
            "-a",
            &paths.disk,
            "--operations",
            "defaults,-user-account"
        ),
        "sysprep reusable image",
    )?;
    let finalizer = paths.dir.join("finalize-image.sh");
    fs::write(&finalizer, FINALIZE_IMAGE).context("write image finalizer")?;
    runner.timed_run(
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
            "--upload",
            format!(
                "{}:/var/tmp/wt-byobu-color",
                paths.dir.join("byobu-color").display()
            ),
            "--run",
            &finalizer
        ),
        "finalize reusable image",
    )?;
    runner.timed_run(
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
    verify_reusable_image_sanitization(runner, paths)?;
    verify_guest_contract(runner, &paths.disk)?;
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
    Ok(())
}

pub(super) fn finalize_development_tools_cache(
    runner: &impl Runner,
    paths: &BuildPaths,
) -> Result<()> {
    println!("Finalizing cached development tools image for reuse (sysprep and sanitization)...");
    runner.timed_run(
        cmd!(
            "sudo",
            "virt-sysprep",
            "-a",
            &paths.disk,
            "--operations",
            "defaults,-user-account"
        ),
        "sysprep cached development tools image",
    )?;
    let finalizer = paths.dir.join("finalize-development-tools-cache.sh");
    fs::write(&finalizer, FINALIZE_DEVELOPMENT_TOOLS_CACHE)
        .context("write development tools cache finalizer")?;
    runner.timed_run(
        cmd!(
            "sudo",
            "virt-customize",
            "-a",
            &paths.disk,
            "--run",
            &finalizer
        ),
        "finalize cached development tools image",
    )?;
    runner.timed_run(
        cmd!(
            "sudo",
            "virt-sysprep",
            "-a",
            &paths.disk,
            "--operations",
            "ssh-hostkeys"
        ),
        "clear cached development tools image SSH host keys",
    )?;
    verify_reusable_image_sanitization(runner, paths)
}

fn verify_reusable_image_sanitization(runner: &impl Runner, paths: &BuildPaths) -> Result<()> {
    let machine_id = runner.text(
        cmd!("sudo", "virt-cat", "-a", &paths.disk, "/etc/machine-id"),
        "verify empty reusable image machine identity",
    )?;
    if !machine_id.is_empty() {
        bail!("reusable image machine identity was not cleared");
    }
    for path in ["/usr/bin/cloud-init", "/etc/cloud", "/var/lib/cloud"] {
        let output = runner.output(cmd!("sudo", "virt-ls", "-a", &paths.disk, path))?;
        if output.status.success() {
            bail!("reusable image still contains removed bootstrap state at {path}");
        }
    }
    let ssh_files = runner.text(
        cmd!("sudo", "virt-ls", "-a", &paths.disk, "/etc/ssh"),
        "inspect reusable image SSH state",
    )?;
    if ssh_files.lines().any(|name| name.starts_with("ssh_host_")) {
        bail!("reusable image still contains SSH host keys");
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
    console: &Path,
) -> Result<ConsoleLog> {
    runner.timed_run(
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
    let directory = server.libvirt.worlds_dir.join(BUILD_NAME);
    if directory.exists() || domain_exists(runner, BUILD_NAME)? {
        bail!("stale image build state exists for {BUILD_NAME}");
    }
    Ok(())
}

pub(super) fn require_clean_publication_state(server: &ServerConfig) -> Result<()> {
    let image = &server.image.path;
    let current_temporary = sibling_temporary(&current_path(image))?;
    let generations = generations_path(image);
    let temporary_generation = if generations.exists() {
        fs::read_dir(&generations)
            .with_context(|| format!("read image generations {}", generations.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.') && name.ends_with(".wt-new"))
            })
    } else {
        None
    };
    if fs::symlink_metadata(&current_temporary).is_ok() || temporary_generation.is_some() {
        bail!(
            "image publication drift: remove abandoned temporary generation state with make nuke"
        );
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
