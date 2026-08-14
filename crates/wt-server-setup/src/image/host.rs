use super::*;
use std::collections::BTreeMap;

const BUILD_NAME: &str = "wt-host-image-build";
const TMUX_CONFIG: &[u8] = include_bytes!("../../../../assets/world/shared/tmux.conf");
const BYOBU_COLOR: &[u8] = include_bytes!("../../../../assets/world/shared/byobu-color");
const HOST_SHELL: &[u8] = include_bytes!("../../../../assets/world/host/shell.sh");
const HOST_IMAGE_BUILD: &[u8] = include_bytes!("../../../../assets/world/host/build-image.sh");
pub(super) const RECIPE_VERSION: u32 = 4;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    recipe_version: u32,
    source_sha256: String,
    config_sha256: String,
    image_sha256: String,
    packages: BTreeMap<String, String>,
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
    let build_dir = server.libvirt.worlds_dir.join(BUILD_NAME);
    if build_dir.exists() || domain_exists(runner, BUILD_NAME)? {
        bail!("stale image build state exists for {BUILD_NAME}");
    }
    fs::create_dir(&build_dir).context("create host image build directory")?;
    let result = (|| {
        fs::set_permissions(&build_dir, fs::Permissions::from_mode(0o2770))
            .context("set host image build directory permissions")?;
        host::ensure_qemu_search_acl(runner, &build_dir)?;
        build_inner(
            runner,
            input,
            server,
            server_bytes,
            source,
            byobu,
            &build_dir,
        )
    })();
    if let Err(primary) = result {
        return match cleanup_failed_build(runner, &build_dir, BUILD_NAME) {
            Ok(()) => Err(primary),
            Err(cleanup) => {
                Err(primary.context(format!("host image build cleanup also failed: {cleanup}")))
            }
        };
    }
    Ok(())
}

fn build_inner(
    runner: &impl Runner,
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    source: &Path,
    byobu: &Path,
    build_dir: &Path,
) -> Result<()> {
    let tmux_config = build_dir.join("tmux.conf");
    let byobu_color = build_dir.join("byobu-color");
    let host_shell = build_dir.join("host-shell.sh");
    fs::write(&tmux_config, TMUX_CONFIG).context("stage host tmux configuration")?;
    fs::write(&byobu_color, BYOBU_COLOR).context("stage host Byobu color setting")?;
    fs::write(&host_shell, HOST_SHELL).context("stage host shell launcher")?;

    let spec = BuildSpec {
        name: BUILD_NAME,
        kind: "host",
        recipe_version: RECIPE_VERSION,
        recipe: HOST_IMAGE_BUILD,
    };
    let extra_inputs = [
        StagedInput {
            source: &tmux_config,
            guest_path: "/var/tmp/wt-tmux.conf",
        },
        StagedInput {
            source: &byobu_color,
            guest_path: "/var/tmp/byobu-color",
        },
        StagedInput {
            source: &host_shell,
            guest_path: "/var/tmp/wt-host-shell",
        },
    ];
    let paths = run_kvm_build(
        runner,
        input,
        server,
        source,
        byobu,
        build_dir,
        &spec,
        &extra_inputs,
    )?;

    let package_output = read_build_file(
        runner,
        &paths.disk,
        &paths.console,
        "/var/lib/wt-image-packages",
        "read installed host package versions",
    )?;
    let packages = parse_packages(&package_output)?;
    validate_packages(&packages)?;

    println!("Sysprepping host image...");
    runner.run(
        cmd!("sudo", "virt-sysprep", "-a", &paths.disk),
        "sysprep host image",
    )?;
    sanitize_reusable_image(runner, &paths.disk)?;

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
        "restore host image build disk ownership",
    )?;
    println!("Compacting host image...");
    runner.run(
        cmd!(
            "qemu-img",
            "convert",
            "-p",
            "-O",
            "qcow2",
            &paths.disk,
            &paths.prepared
        ),
        "compact host image",
    )?;
    runner.run(
        cmd!("qemu-img", "check", &paths.prepared),
        "check host image",
    )?;

    let manifest = Manifest {
        version: IMAGE_MANIFEST_VERSION,
        recipe_version: RECIPE_VERSION,
        source_sha256: input.source_sha256().to_ascii_lowercase(),
        config_sha256: image_config_sha(server_bytes, input),
        image_sha256: sha_file(&paths.prepared)?,
        packages,
        byobu: recipe::BYOBU_VERSION.to_owned(),
        tmux: recipe::TMUX_VERSION.to_owned(),
        ghostty_terminfo_sha256: recipe::GHOSTTY_TERMINFO_SHA256.to_owned(),
    };
    let manifest_path = manifest_path(&server.image.host_path);
    let publication = stage_publication(
        runner,
        &paths.prepared,
        &server.image.host_path,
        &manifest_path,
        &manifest,
    )?;
    fs::remove_dir_all(&paths.dir).context("remove host image build directory")?;
    publication.publish(runner)
}

fn parse_packages(text: &str) -> Result<BTreeMap<String, String>> {
    let mut packages = BTreeMap::new();
    for line in text.lines() {
        let (name, version) = line
            .split_once('\t')
            .with_context(|| format!("malformed host package version line: {line:?}"))?;
        if name.is_empty()
            || version.is_empty()
            || packages.insert(name.into(), version.into()).is_some()
        {
            bail!("invalid host package version line: {line:?}");
        }
    }
    Ok(packages)
}

fn validate_packages(packages: &BTreeMap<String, String>) -> Result<()> {
    let expected = ["byobu", "openssh-server", "qemu-guest-agent", "tmux"];
    if packages.keys().map(String::as_str).ne(expected) {
        bail!("installed host package manifest differs from policy");
    }
    if packages["byobu"] != recipe::BYOBU_VERSION {
        bail!(
            "installed host Byobu version is {}; expected {}",
            packages["byobu"],
            recipe::BYOBU_VERSION
        );
    }
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
    validate_packages(&manifest.packages)
        .context("installed host image package provenance differs")?;
    require_sha(
        &server.image.host_path,
        &manifest.image_sha256,
        "installed host image",
    )
}
