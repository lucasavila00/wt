use std::path::Path;
use std::process::{Command, Output};

#[test]
fn checkout_and_commit_hints_explain_the_environment() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "-b", "wrong"]);
    for (key, value) in [
        ("wt.project", "git@github.com:group/project.git"),
        ("wt.base", "main"),
        ("wt.prefix", "wt/"),
    ] {
        git(temp.path(), &["config", key, value]);
    }
    insta::assert_snapshot!("checkout_hint", stderr(run_hint(temp.path(), "checkout")));
    git(temp.path(), &["branch", "-m", "wt/fix-login"]);
    insta::assert_snapshot!("commit_hint", stderr(run_hint(temp.path(), "commit")));
}

fn run_hint(repository: &Path, mode: &str) -> Output {
    Command::new("/bin/sh")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/world/devcontainer/agent-git-hint.sh"
        ))
        .arg(mode)
        .current_dir(repository)
        .output()
        .unwrap()
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(output));
}

fn stderr(output: Output) -> String {
    String::from_utf8(output.stderr).unwrap()
}
