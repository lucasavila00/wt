use std::path::Path;
use std::process::Command;

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git: {error}"));
    if !output.status.success() {
        panic!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .expect("git output is not UTF-8")
        .trim()
        .to_owned()
}

fn main() {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let root = Path::new(&manifest)
        .parent()
        .and_then(Path::parent)
        .expect("wt-api is not inside the WT repository");
    let commit = git(root, &["rev-parse", "--verify", "HEAD"]);
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "WT Git commit is not a full lowercase SHA-1: {commit}"
    );

    let git_path = |value: String| {
        let path = Path::new(&value);
        if path.is_absolute() {
            path.to_owned()
        } else {
            root.join(path)
        }
    };
    let head = git_path(git(root, &["rev-parse", "--git-path", "HEAD"]));
    println!("cargo:rerun-if-changed={}", head.display());
    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["symbolic-ref", "HEAD"])
        .output()
    {
        if output.status.success() {
            let reference = String::from_utf8(output.stdout)
                .expect("git symbolic ref is not UTF-8")
                .trim()
                .to_owned();
            let reference = git_path(git(root, &["rev-parse", "--git-path", &reference]));
            println!("cargo:rerun-if-changed={}", reference.display());
        }
    }
    println!("cargo:rustc-env=WT_GIT_COMMIT={commit}");
}
