use super::*;
use std::collections::BTreeMap;

pub(super) const BUILD_NAME: &str = "wt-host-image-build";
const HOST_SHELL: &[u8] = include_bytes!("../../../../../../assets/world/host/shell.sh");
const HOST_IMAGE_BUILD: &[u8] =
    include_bytes!("../../../../../../assets/world/host/build-image.sh");
const HOST_PREPARE: &[u8] = include_bytes!("../../../../../../assets/world/host/prepare.sh");
const HOST_INSPECT: &[u8] = include_bytes!("../../../../../../assets/world/host/inspect.sh");
const HOST_CLOUD_INIT: &[u8] = include_bytes!("../../../../../../assets/world/host/cloud-init.sh");
const HOST_SETUP: &[u8] = include_bytes!("../../../../../../assets/world/host/setup.sh");
const HOST_DEFER_INIT: &[u8] =
    include_bytes!("../../../../../../assets/world/host/defer-init.yaml");
const HOST_CLOUD_CONFIG: &[u8] =
    include_bytes!("../../../../../../assets/world/host/cloud-config.conf");
const HOST_CLOUD_FINAL: &[u8] =
    include_bytes!("../../../../../../assets/world/host/cloud-final.conf");
const HOST_SETUP_SERVICE: &[u8] =
    include_bytes!("../../../../../../assets/world/host/setup.service");
const HOST_INPUTS: &[(&str, &str, &[u8])] = &[
    ("host-shell", "/var/tmp/wt-host-shell", HOST_SHELL),
    ("host-prepare", "/var/tmp/wt-host-prepare", HOST_PREPARE),
    ("host-inspect", "/var/tmp/wt-host-inspect", HOST_INSPECT),
    (
        "host-cloud-init",
        "/var/tmp/wt-host-cloud-init",
        HOST_CLOUD_INIT,
    ),
    ("host-setup", "/var/tmp/wt-host-setup", HOST_SETUP),
    (
        "host-defer-init",
        "/var/tmp/wt-host-defer-init",
        HOST_DEFER_INIT,
    ),
    (
        "host-cloud-config",
        "/var/tmp/wt-host-cloud-config",
        HOST_CLOUD_CONFIG,
    ),
    (
        "host-cloud-final",
        "/var/tmp/wt-host-cloud-final",
        HOST_CLOUD_FINAL,
    ),
    (
        "host-setup-service",
        "/var/tmp/wt-host-setup-service",
        HOST_SETUP_SERVICE,
    ),
];
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    source_sha256: String,
    config_sha256: String,
    inputs: BTreeMap<String, String>,
    image_sha256: String,
    packages: BTreeMap<String, String>,
    byobu: String,
    tmux: String,
    tmux_sha256: String,
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
    match installed_image_state(
        server.image.host_path.exists(),
        manifest_path.exists(),
        || verify(input, server, server_bytes, &manifest_path),
    ) {
        InstalledImageState::Reusable => Ok(()),
        InstalledImageState::Missing => build(runner, input, server, server_bytes, source, byobu),
        InstalledImageState::Replace(reason) => {
            println!("Replacing the installed host golden image: {reason}");
            println!("Existing worlds use independent disks and are unaffected.");
            build(runner, input, server, server_bytes, source, byobu)
        }
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
    let context = BuildContext {
        runner,
        input,
        server,
        source,
        byobu,
    };
    let result = (|| {
        fs::set_permissions(&build_dir, fs::Permissions::from_mode(0o2770))
            .context("set host image build directory permissions")?;
        host::ensure_qemu_search_acl(runner, &build_dir)?;
        build_inner(&context, server_bytes, &build_dir)
    })();
    if let Err(primary) = result {
        let primary = attach_console_tail(primary, &build_dir);
        return match cleanup_failed_build(runner, &build_dir, BUILD_NAME) {
            Ok(()) => Err(primary),
            Err(cleanup) => {
                Err(primary.context(format!("host image build cleanup also failed: {cleanup}")))
            }
        };
    }
    Ok(())
}

fn build_inner<R: Runner>(
    context: &BuildContext<'_, R>,
    server_bytes: &[u8],
    build_dir: &Path,
) -> Result<()> {
    let runner = context.runner;
    let input = context.input;
    let server = context.server;
    let staged_paths = HOST_INPUTS
        .iter()
        .map(|(name, _, bytes)| {
            let path = build_dir.join(name);
            fs::write(&path, bytes).context("stage host image input")?;
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()?;

    let spec = BuildSpec {
        name: BUILD_NAME,
        kind: ImageKind::Host,
        recipe: HOST_IMAGE_BUILD,
    };
    let extra_inputs = staged_paths
        .iter()
        .zip(HOST_INPUTS)
        .map(|(source, (_, guest_path, _))| StagedInput { source, guest_path })
        .collect::<Vec<_>>();
    let paths = run_kvm_build(context, build_dir, &spec, &extra_inputs)?;

    let package_output = read_build_file(
        runner,
        &paths.disk,
        &paths.console,
        "/var/lib/wt-image-packages",
        "read installed host package versions",
    )?;
    let packages = parse_packages(&package_output)?;
    validate_packages(&packages)?;

    let tmux_sha256 = finalize_reusable_image(runner, &paths)?;
    let finalized_package_output = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            &paths.disk,
            "/var/lib/wt-image-packages"
        ),
        "revalidate finalized host package versions",
    )?;
    let finalized_packages = parse_packages(&finalized_package_output)?;
    validate_packages(&finalized_packages)?;
    if finalized_packages != packages {
        bail!("finalized host package versions changed during sanitization");
    }

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
        source_sha256: input.source_sha256().to_ascii_lowercase(),
        config_sha256: image_config_sha(server_bytes, input),
        inputs: host_input_hashes(&spec),
        image_sha256: sha_file(&paths.prepared)?,
        packages,
        byobu: recipe::BYOBU_VERSION.to_owned(),
        tmux: recipe::TMUX_VERSION.to_owned(),
        tmux_sha256,
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

pub(super) fn verify(
    input: &InstallInput,
    server: &ServerConfig,
    server_bytes: &[u8],
    manifest_path: &Path,
) -> Result<()> {
    require_named_file(&server.image.host_path, "libvirt-qemu", "kvm", 0o644)?;
    require_root_file(manifest_path, 0o644)?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.source_sha256 != input.source_sha256().to_ascii_lowercase()
        || manifest.config_sha256 != image_config_sha(server_bytes, input)
        || manifest.inputs
            != host_input_hashes(&BuildSpec {
                name: BUILD_NAME,
                kind: ImageKind::Host,
                recipe: HOST_IMAGE_BUILD,
            })
        || manifest.byobu != recipe::BYOBU_VERSION
        || manifest.tmux != recipe::TMUX_VERSION
        || !is_sha256(&manifest.tmux_sha256)
        || manifest.ghostty_terminfo_sha256 != recipe::GHOSTTY_TERMINFO_SHA256
    {
        bail!("provenance does not match the current source or install input");
    }
    validate_packages(&manifest.packages)
        .context("installed host image package provenance differs")?;
    require_sha(
        &server.image.host_path,
        &manifest.image_sha256,
        "installed host image",
    )
}

fn host_input_hashes(spec: &BuildSpec<'_>) -> BTreeMap<String, String> {
    let inputs = HOST_INPUTS
        .iter()
        .map(|(_, guest_path, bytes)| (*guest_path, *bytes))
        .collect::<Vec<_>>();
    staged_input_hashes(spec, &inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_package_manifest_is_exact() {
        let text = format!(
            "byobu\t{}\nopenssh-server\t1\nqemu-guest-agent\t2\ntmux\t3\n",
            recipe::BYOBU_VERSION
        );
        let packages = parse_packages(&text).unwrap();
        validate_packages(&packages).unwrap();

        let mut unexpected = packages;
        unexpected.insert("git".to_owned(), "1".to_owned());
        assert!(validate_packages(&unexpected).is_err());
    }
}
