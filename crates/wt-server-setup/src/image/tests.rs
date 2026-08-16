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
        inputs: BTreeMap::new(),
        golden_sha256: "golden".to_owned(),
        tmux_sha256: "tmux".to_owned(),
        packages: [("tmux".to_owned(), "3.4-1".to_owned())].into(),
        devcontainer_cli: wt_devcontainer::DEVCONTAINER_CLI_VERSION.to_owned(),
    };

    let json = serde_json::to_value(manifest).unwrap();
    assert_eq!(json["version"], 2);
    assert_eq!(json["recipe_version"], 1);
    assert_eq!(json["packages"]["tmux"], "3.4-1");
}

#[test]
fn provenance_drift_has_copyable_recovery_commands() {
    let message =
        provenance_drift_message("devcontainer", Path::new("/srv/wt/config's/server.toml"));

    insta::assert_snapshot!(message, @r###"
    installed devcontainer golden image provenance does not match the current source or install input. Provenance covers the pinned source image, image-build configuration, and checked-in recipe assets.
    The installed images were left unchanged. Rebuild both golden images explicitly, then retry installation:
      scripts/prepare-image --config '/srv/wt/config'"'"'s/server.toml'
      scripts/install-server --config '/srv/wt/config'"'"'s/server.toml'
    If rebuilding reports active WT domains, stop those worlds and rerun the command.
    "###);
}

#[test]
fn result_marker_requires_root_0644_metadata() {
    validate_result_metadata("- 0644 46 0 0 /var/lib/wt-image-result").unwrap();
    assert!(validate_result_metadata("- 0600 46 0 0 /var/lib/wt-image-result").is_err());
    assert!(validate_result_metadata("- 0644 46 1000 1000 /var/lib/wt-image-result").is_err());
}

#[test]
fn image_build_lock_is_exclusive_and_released() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lock");
    let lock = BuildLock::acquire_at(&path).unwrap();
    assert!(BuildLock::acquire_at(&path).is_err());
    drop(lock);
    BuildLock::acquire_at(&path).unwrap();
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
fn shell_trace_is_not_a_phase_marker() {
    let mut pending = Vec::new();
    assert!(extract_phase_markers(
        &mut pending,
        b"[  1.0] cloud-init: + echo WT_IMAGE_PHASE=installing packages\n"
    )
    .is_empty());
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
