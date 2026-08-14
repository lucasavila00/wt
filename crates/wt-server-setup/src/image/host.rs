use super::*;

const TMUX_CONFIG: &[u8] = include_bytes!("../../../../assets/world/shared/tmux.conf");
const BYOBU_COLOR: &[u8] = include_bytes!("../../../../assets/world/shared/byobu-color");
const HOST_SHELL: &[u8] = include_bytes!("../../../../assets/world/host/shell.sh");
const HOST_RESOLVER_PATH: &str = "/run/systemd/resolve/resolv.conf";
const RECIPE_VERSION: u32 = 2;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    recipe_version: u32,
    source_sha256: String,
    config_sha256: String,
    image_sha256: String,
    byobu: String,
    tmux: String,
    ghostty_terminfo_sha256: String,
}

pub(super) fn ensure(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
    byobu: &Path,
) -> Result<()> {
    let manifest_path = manifest_path(&server.image.host_path);
    match (server.image.host_path.exists(), manifest_path.exists()) {
        (true, true) => verify(input, server, server_bytes, &manifest_path),
        (false, false) => build(runner, input, server, server_bytes, source, byobu),
        _ => bail!("host image drift: image and manifest must both exist or both be absent"),
    }
}

pub(super) fn build(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
    byobu: &Path,
) -> Result<()> {
    let build = server.libvirt.worlds_dir.join("wt-host-image-build.qcow2");
    let terminal = server.libvirt.worlds_dir.join("wt-host-terminal-assets");
    if build.exists() {
        bail!("stale host image build exists: {}", build.display());
    }
    if terminal.exists() {
        bail!("stale host terminal assets exist: {}", terminal.display());
    }
    fs::create_dir(&terminal).context("create host terminal asset directory")?;
    let result = (|| {
        println!("Preparing dedicated host image and shared terminal assets...");
        extract_terminal_files(runner, server, &terminal)?;
        fs::write(terminal.join("wt-tmux.conf"), TMUX_CONFIG)?;
        fs::write(terminal.join("byobu-color"), BYOBU_COLOR)?;
        fs::write(terminal.join("wt-host-shell"), HOST_SHELL)?;
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
        println!("Installing host image packages and terminal assets...");
        customize(runner, &build, byobu, &terminal)?;
        println!("Host image packages and terminal assets installed.");
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
        let manifest = Manifest {
            version: IMAGE_MANIFEST_VERSION,
            recipe_version: RECIPE_VERSION,
            source_sha256: input.source_sha256().to_ascii_lowercase(),
            config_sha256: image_config_sha(server_bytes, input),
            image_sha256: sha_file(&build)?,
            byobu: recipe::BYOBU_VERSION.to_owned(),
            tmux: recipe::TMUX_VERSION.to_owned(),
            ghostty_terminfo_sha256: recipe::GHOSTTY_TERMINFO_SHA256.to_owned(),
        };
        publish(runner, server, &build, &manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&build);
    }
    let cleanup = runner.run(
        cmd!("sudo", "rm", "-rf", "--", &terminal),
        "remove staged host terminal assets",
    );
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(primary.context(format!("host terminal cleanup also failed: {cleanup}")))
        }
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn extract_terminal_files(
    runner: &impl Runner,
    server: &ServerConfig,
    terminal: &Path,
) -> Result<()> {
    for (source, action) in [
        ("/usr/bin/tmux", "extract pinned tmux for host image"),
        (
            "/usr/share/terminfo/g/ghostty",
            "extract Ghostty terminfo for host image",
        ),
        (
            "/usr/share/terminfo/x/xterm-ghostty",
            "extract xterm-Ghostty terminfo for host image",
        ),
    ] {
        runner.run(
            cmd!(
                "sudo",
                "virt-copy-out",
                "-a",
                &server.image.devcontainer_path,
                source,
                terminal
            ),
            action,
        )?;
    }
    let user = User::from_uid(Uid::effective())
        .context("look up server user")?
        .context("server user does not exist")?;
    runner.run(
        cmd!(
            "sudo",
            "chown",
            "-R",
            format!("{}:{}", user.uid.as_raw(), user.gid.as_raw()),
            terminal
        ),
        "own extracted host terminal assets",
    )
}

fn customize(runner: &impl Runner, build: &Path, byobu: &Path, terminal: &Path) -> Result<()> {
    let resolver = Path::new(HOST_RESOLVER_PATH);
    if !resolver.is_file() {
        bail!(
            "host image build resolver is unavailable at {HOST_RESOLVER_PATH}; systemd-resolved must be active"
        );
    }
    runner.run(
        cmd!(
            "sudo",
            "virt-customize",
            "-a",
            build,
            "--network",
            "--run-command",
            "rm -f /etc/resolv.conf",
            "--upload",
            format!("{}:/etc/resolv.conf", resolver.display()),
            "--run-command",
            install_prerequisites_command(),
            "--upload",
            format!("{}:/var/tmp/wt-byobu.deb", byobu.display()),
            "--upload",
            format!("{}:/var/tmp/wt-tmux", terminal.join("tmux").display()),
            "--upload",
            format!("{}:/var/tmp/ghostty", terminal.join("ghostty").display()),
            "--upload",
            format!(
                "{}:/var/tmp/xterm-ghostty",
                terminal.join("xterm-ghostty").display()
            ),
            "--upload",
            format!(
                "{}:/var/tmp/wt-tmux.conf",
                terminal.join("wt-tmux.conf").display()
            ),
            "--upload",
            format!(
                "{}:/var/tmp/byobu-color",
                terminal.join("byobu-color").display()
            ),
            "--upload",
            format!(
                "{}:/var/tmp/wt-host-shell",
                terminal.join("wt-host-shell").display()
            ),
            "--run-command",
            install_command()
        ),
        "install host image prerequisites",
    )
}

fn install_prerequisites_command() -> &'static str {
    "export DEBIAN_FRONTEND=noninteractive; log=/var/tmp/wt-host-image-apt.log; attempt=1; while ! (apt-get update && apt-get install -y --no-install-recommends openssh-server qemu-guest-agent) >\"$log\" 2>&1; do if test \"$attempt\" -ge 30; then echo \"wt-host-image: package installation failed after $attempt attempts; final apt output follows\" >&2; cat \"$log\" >&2; exit 1; fi; echo \"wt-host-image: package installation attempt $attempt/30 failed; retrying in 2 seconds\" >&2; attempt=$((attempt + 1)); sleep 2; done; echo \"wt-host-image: packages installed on attempt $attempt/30\" >&2; rm -f \"$log\" /etc/resolv.conf; ln -s ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf"
}

fn install_command() -> String {
    format!(
        "printf '%s  %s\\n' {} /var/tmp/wt-byobu.deb | sha256sum --check --strict && apt-get install -y --no-install-recommends /var/tmp/wt-byobu.deb && test \"$(dpkg-query -W -f='${{Version}}' byobu)\" = '{}' && install -m 0755 /var/tmp/wt-tmux /usr/bin/tmux && test \"$(/usr/bin/tmux -V)\" = 'tmux {}' && install -d -m 0755 /usr/share/terminfo/g /usr/share/terminfo/x /usr/local/share /etc/skel/.byobu && install -m 0644 /var/tmp/ghostty /usr/share/terminfo/g/ghostty && install -m 0644 /var/tmp/xterm-ghostty /usr/share/terminfo/x/xterm-ghostty && printf '%s  %s\\n' {} /usr/share/terminfo/g/ghostty | sha256sum --check --strict && cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty && TERM=ghostty tput colors > /dev/null && TERM=xterm-ghostty tput colors > /dev/null && install -m 0644 /var/tmp/wt-tmux.conf /usr/local/share/wt-tmux.conf && install -m 0644 /var/tmp/wt-tmux.conf /etc/skel/.byobu/.tmux.conf && install -m 0644 /var/tmp/byobu-color /etc/skel/.byobu/color && install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell && rm -f /var/tmp/wt-byobu.deb /var/tmp/wt-tmux /var/tmp/ghostty /var/tmp/xterm-ghostty /var/tmp/wt-tmux.conf /var/tmp/byobu-color /var/tmp/wt-host-shell && systemctl enable qemu-guest-agent.service ssh.service",
        recipe::BYOBU_SHA256,
        recipe::BYOBU_VERSION,
        recipe::TMUX_VERSION,
        recipe::GHOSTTY_TERMINFO_SHA256,
    )
}

fn publish(
    runner: &impl Runner,
    server: &ServerConfig,
    prepared: &Path,
    manifest: &Manifest,
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

fn verify(
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    require_named_file(&server.image.host_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.version != IMAGE_MANIFEST_VERSION
        || manifest.recipe_version != RECIPE_VERSION
        || manifest.source_sha256 != input.source_sha256().to_ascii_lowercase()
        || manifest.config_sha256 != image_config_sha(server_bytes, input)
        || manifest.byobu != recipe::BYOBU_VERSION
        || manifest.tmux != recipe::TMUX_VERSION
        || manifest.ghostty_terminfo_sha256 != recipe::GHOSTTY_TERMINFO_SHA256
    {
        bail!("installed host image provenance differs from the current install input");
    }
    require_sha(
        &server.image.host_path,
        &manifest.image_sha256,
        "installed host image",
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_package_install_retries_transient_failures() {
        insta::assert_snapshot!(super::install_prerequisites_command(), @"export DEBIAN_FRONTEND=noninteractive; log=/var/tmp/wt-host-image-apt.log; attempt=1; while ! (apt-get update && apt-get install -y --no-install-recommends openssh-server qemu-guest-agent) >\"$log\" 2>&1; do if test \"$attempt\" -ge 30; then echo \"wt-host-image: package installation failed after $attempt attempts; final apt output follows\" >&2; cat \"$log\" >&2; exit 1; fi; echo \"wt-host-image: package installation attempt $attempt/30 failed; retrying in 2 seconds\" >&2; attempt=$((attempt + 1)); sleep 2; done; echo \"wt-host-image: packages installed on attempt $attempt/30\" >&2; rm -f \"$log\" /etc/resolv.conf; ln -s ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf");
    }
}
