use super::*;

fn output(repository: &Path, args: &[&str], input: &[u8]) -> Vec<u8> {
    let mut child = git(repository)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn oid(repository: &Path, args: &[&str], input: &[u8]) -> String {
    String::from_utf8(output(repository, args, input))
        .unwrap()
        .trim()
        .to_owned()
}

fn commit(repository: &Path, parent: Option<&str>, text: &str) -> (String, String) {
    let blob = oid(
        repository,
        &["hash-object", "-w", "--stdin"],
        text.as_bytes(),
    );
    let tree = oid(
        repository,
        &["mktree"],
        format!("100644 blob {blob}\tfile\n").as_bytes(),
    );
    let mut args = vec!["commit-tree", &tree];
    if let Some(parent) = parent {
        args.extend(["-p", parent]);
    }
    (oid(repository, &args, b"test\n"), blob)
}

fn commands(updates: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut commands = Vec::new();
    for (old, new, reference) in updates {
        write_packet(
            &mut commands,
            format!("{old} {new} {reference}\n").as_bytes(),
        )
        .unwrap();
    }
    commands.extend_from_slice(b"0000");
    commands
}

fn pack(repository: &Path, new: &str, old: Option<&str>) -> Vec<u8> {
    let revisions = match old {
        Some(old) => format!("{new}\n^{old}\n"),
        None => format!("{new}\n"),
    };
    output(
        repository,
        &["pack-objects", "--stdout", "--revs", "--thin"],
        revisions.as_bytes(),
    )
}

fn validates_history(format: &str) {
    let temp = tempfile::tempdir().unwrap();
    let seed = temp.path().join("seed.git");
    let upstream = temp.path().join("upstream.git");
    for repository in [&seed, &upstream] {
        run(git(repository).args([
            "init",
            "--bare",
            "--template=",
            &format!("--object-format={format}"),
        ]))
        .unwrap();
    }
    let target = GitTarget::Local {
        repositories: temp.path(),
        path: "upstream.git",
    };
    let text = (0..10000)
        .map(|n| format!("line {n}\n"))
        .collect::<String>();
    let (first, _) = commit(&seed, None, &text);
    let (second, _) = commit(&seed, Some(&first), &format!("{text}new\n"));
    let zero = "0".repeat(first.len());

    // A new branch in a completely empty upstream.
    let mut incoming = std::io::Cursor::new(pack(&seed, &first, None));
    validated_pack(
        &mut incoming,
        &target,
        &commands(&[(&zero, &first, "refs/heads/wt/new")]),
    )
    .unwrap();

    output(&seed, &["update-ref", "refs/heads/main", &first], b"");
    output(&seed, &["push", upstream.to_str().unwrap(), "main"], b"");
    let thin = pack(&seed, &second, Some(&first));
    // This pack really needs an upstream delta base.
    let isolated = temp.path().join("isolated.git");
    run(git(&isolated).args([
        "init",
        "--bare",
        "--template=",
        &format!("--object-format={format}"),
    ]))
    .unwrap();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&thin).unwrap();
    file.rewind().unwrap();
    assert!(!index_pack(&isolated, file.into())
        .unwrap()
        .0
        .wait()
        .unwrap()
        .success());

    let mut incoming = std::io::Cursor::new(thin);
    let validated = validated_pack(
        &mut incoming,
        &target,
        &commands(&[
            (&first, &second, "refs/heads/main"),
            (&zero, &second, "refs/heads/wt/new"),
        ]),
    )
    .unwrap();
    // The original validated thin pack remains valid to an upstream possessing
    // its delta bases, and staging has not changed any upstream refs.
    assert!(index_pack(&upstream, validated.into())
        .unwrap()
        .0
        .wait()
        .unwrap()
        .success());
    assert_eq!(oid(&upstream, &["rev-parse", "main"], b""), first);
}

#[test]
fn stages_sha1_thin_packs_and_branch_creation() {
    validates_history("sha1");
}

#[test]
fn stages_sha256_thin_packs_and_branch_creation() {
    validates_history("sha256");
}

#[test]
fn rejects_rewrites_noncommits_and_corrupt_objects() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("upstream.git");
    run(git(&repository).args(["init", "--bare", "--template="])).unwrap();
    let (first, blob) = commit(&repository, None, "first\n");
    let (second, _) = commit(&repository, Some(&first), "second\n");
    output(
        &repository,
        &["update-ref", "refs/heads/main", &second],
        b"",
    );
    let target = GitTarget::Local {
        repositories: temp.path(),
        path: "upstream.git",
    };
    let request = commands(&[(&second, &first, "refs/heads/main")]);
    let mut incoming = std::io::Cursor::new(pack(&repository, &first, Some(&second)));
    insta::assert_snapshot!(validated_pack(&mut incoming, &target, &request).unwrap_err().to_string(), @"non-fast-forward update to `refs/heads/main` rejected; gateway preserves history");

    let request = commands(&[(&second, &blob, "refs/heads/main")]);
    let mut incoming = std::io::Cursor::new(pack(&repository, &blob, None));
    insta::assert_snapshot!(validated_pack(&mut incoming, &target, &request).unwrap_err().to_string(), @"branch `refs/heads/main` must point to a commit");

    let mut corrupt = pack(&repository, &second, None);
    *corrupt.last_mut().unwrap() ^= 1;
    let request = commands(&[(&first, &second, "refs/heads/main")]);
    insta::assert_snapshot!(validated_pack(&mut std::io::Cursor::new(corrupt), &target, &request).unwrap_err().to_string(), @"invalid or incomplete Git pack");
    assert_eq!(oid(&repository, &["rev-parse", "main"], b""), second);
}
