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
        commit: wt_control_protocol::GIT_COMMIT_SHA.to_owned(),
        guest_identity: wt_retained_worlds::GUEST_IDENTITY,
        golden_sha256: "golden".to_owned(),
        packages: [("tmux".to_owned(), "3.4-1".to_owned())].into(),
        development_tools: Default::default(),
    };

    let json = serde_json::to_value(manifest).unwrap();
    assert_eq!(json["packages"]["tmux"], "3.4-1");
    assert_eq!(json["commit"], wt_control_protocol::GIT_COMMIT_SHA);
    assert_eq!(json["guest_identity"]["uid"], 1001);
    assert_eq!(json["guest_identity"]["gid"], 1001);
}

#[test]
fn development_tools_cache_identity_tracks_its_source_and_policy() {
    let identity = cache::identity(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        32,
        "wt:wt:1001:1001:/home/wt",
        "24.19.0",
    );

    assert_ne!(
        identity,
        cache::identity(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            32,
            "wt:wt:1001:1001:/home/wt",
            "24.19.0",
        )
    );
    assert_ne!(
        identity,
        cache::identity(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            32,
            "wt:wt:1002:1002:/home/wt",
            "24.19.0",
        )
    );
    assert_ne!(
        identity,
        cache::identity(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            64,
            "wt:wt:1001:1001:/home/wt",
            "24.19.0",
        )
    );
    assert_ne!(
        identity,
        cache::identity(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            32,
            "wt:wt:1001:1001:/home/wt",
            "24.20.0",
        )
    );
    assert_eq!(identity.len(), 64);
    assert!(identity.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn development_tools_cache_finalizer_needs_no_final_image_assets() {
    let finalizer = std::str::from_utf8(builder::FINALIZE_DEVELOPMENT_TOOLS_CACHE).unwrap();

    assert!(finalizer.contains("/var/lib/wt-image-development-tools"));
    for final_asset in ["wt-tmux", "codex", "wt-retained"] {
        assert!(!finalizer.contains(final_asset));
    }
}

#[test]
fn development_tools_cache_manifest_is_structured() {
    let manifest = cache::DevelopmentToolsCacheManifest {
        identity: "policy".to_owned(),
        sha256: "image".to_owned(),
    };

    assert_eq!(
        serde_json::to_value(manifest).unwrap(),
        serde_json::json!({
            "identity": "policy",
            "sha256": "image",
        })
    );
}

#[test]
fn image_publication_rejects_a_mismatched_guest_identity() {
    struct UnusedRunner;

    impl Runner for UnusedRunner {
        fn output(&self, _command: std::process::Command) -> Result<std::process::Output> {
            unreachable!()
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let prepared = directory.path().join("prepared.qcow2");
    let destination = directory.path().join("retained.qcow2");
    let manifest = ImageManifest {
        commit: wt_control_protocol::GIT_COMMIT_SHA.to_owned(),
        guest_identity: wt_retained_worlds::GuestIdentity {
            uid: 1000,
            gid: 1000,
        },
        golden_sha256: "golden".to_owned(),
        packages: Default::default(),
        development_tools: Default::default(),
    };

    let error = stage_publication(&UnusedRunner, &prepared, &destination, &manifest)
        .err()
        .unwrap();

    insta::assert_snapshot!(error.to_string(), @"retained image guest identity mismatch: expected UID/GID 1001:1001, got 1000:1000");
    assert!(!prepared.with_extension("manifest.json").exists());
}

#[test]
fn image_reuse_requires_the_current_commit() {
    require_current_commit(wt_control_protocol::GIT_COMMIT_SHA).unwrap();
    insta::assert_snapshot!(
        require_current_commit("different-commit").unwrap_err().to_string(),
        @"image commit does not match the current WT commit"
    );
}

#[test]
fn retained_image_owns_static_guest_binaries() {
    let inputs = GUEST_BINARY_INPUTS
        .iter()
        .map(|(name, path)| format!("{name}\t{path}"))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(inputs, @r###"
    wt-agent-tool-gateway-relay	/var/tmp/wt-agent-tool-gateway-relay
    git-remote-wt-agent	/var/tmp/wt-git-remote-agent
    wt-tools	/var/tmp/wt-tools
    wt-codex-integration	/var/tmp/wt-codex-integration
    "###);
}

#[test]
fn installed_image_drift_is_replaced_automatically() {
    assert_eq!(
        installed_image_state(false, false, || unreachable!()),
        InstalledImageState::Missing
    );
    assert_eq!(
        installed_image_state(true, true, || Ok(())),
        InstalledImageState::Reusable
    );
    assert_eq!(
        installed_image_state(true, true, || anyhow::bail!("recipe changed")),
        InstalledImageState::Replace("recipe changed".to_owned())
    );
    assert_eq!(
        installed_image_state(true, false, || unreachable!()),
        InstalledImageState::Replace(
            "the image and provenance manifest are not a complete pair".to_owned()
        )
    );
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
        b"[  1.0] bootstrap: + echo WT_IMAGE_PHASE=installing packages\n"
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
    let message = progress_message(
        "Retained",
        "installing base operating-system packages",
        Duration::from_secs(60),
    );
    insta::assert_snapshot!(message, @"Retained image build: installing base operating-system packages (elapsed=60s)");
}

#[test]
fn development_tool_install_reports_individual_progress() {
    let scripts = [
        builder::DEVELOPMENT_TOOLS_CACHE_BUILD,
        builder::INSTALL_DEVELOPMENT_TOOLS,
    ];
    let script_text = scripts
        .iter()
        .map(|script| std::str::from_utf8(script).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let phases = script_text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("phase \"")?.strip_suffix('"'))
        .filter(|phase| {
            phase.contains("development")
                || phase.contains("Go")
                || phase.contains("Rust")
                || phase.contains("Node.js")
                || phase.contains("Python")
                || phase.contains("Docker")
        })
        .collect::<Vec<_>>();

    insta::assert_snapshot!(phases.join("\n"), @r###"
    installing cached operating-system and development packages
    installing cached development tools
    installing Go
    installing Rust and Cargo
    installing Node.js and nvm
    installing Python and uv
    configuring Node.js command path
    configuring Docker for the retained-world user
    recording installed development-tool versions
    "###);
}
