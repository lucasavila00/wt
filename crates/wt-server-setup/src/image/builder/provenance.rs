use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wt_setup_core::{sudo_install, sudo_install_owned, sudo_move};

pub(in crate::image) fn staged_input_hashes(
    spec: &BuildSpec<'_>,
    extra_inputs: &[(&str, &[u8])],
) -> BTreeMap<String, String> {
    let environment = recipe::BuildEnvironment {
        kind: spec.kind.as_str(),
        tmux_config_sha256: &sha_bytes(TMUX_CONFIG),
        byobu_color_sha256: &sha_bytes(BYOBU_COLOR),
        access_sha256: &sha_bytes(CONFIGURE_ACCESS),
        git_author_sha256: &sha_bytes(CONFIGURE_GIT_AUTHOR),
        agent_git_sha256: &sha_bytes(INSTALL_AGENT_GIT),
        mount_codex_sha256: &sha_bytes(MOUNT_CODEX),
    }
    .render();
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
            "/var/tmp/wt-install-codex.sh".to_owned(),
            sha_bytes(INSTALL_CODEX),
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
            "/var/tmp/wt-retained-access".to_owned(),
            sha_bytes(CONFIGURE_ACCESS),
        ),
        (
            "/var/tmp/wt-retained-git-author".to_owned(),
            sha_bytes(CONFIGURE_GIT_AUTHOR),
        ),
        (
            "/var/tmp/wt-retained-agent-git".to_owned(),
            sha_bytes(INSTALL_AGENT_GIT),
        ),
        (
            "/var/tmp/wt-retained-mount-codex".to_owned(),
            sha_bytes(MOUNT_CODEX),
        ),
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

pub(in crate::image) struct PendingPublication {
    image_temporary: PathBuf,
    manifest_temporary: PathBuf,
    image_destination: PathBuf,
    manifest_destination: PathBuf,
}

impl PendingPublication {
    pub(in crate::image) fn publish(self, runner: &impl Runner) -> Result<()> {
        sudo_move(runner, &self.image_temporary, &self.image_destination)?;
        sudo_move(runner, &self.manifest_temporary, &self.manifest_destination)
    }
}

pub(in crate::image) fn stage_publication<T: Serialize>(
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

pub(in crate::image) fn image_config_sha(server_bytes: &[u8], input: &InstallInput) -> String {
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

pub(in crate::image) fn sha_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
