use std::process::Command;

fn git_output(arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run git {}: {error}", arguments.join(" ")));
    assert!(
        output.status.success(),
        "git {} failed",
        arguments.join(" ")
    );
    String::from_utf8(output.stdout)
        .expect("Git output is UTF-8")
        .trim()
        .to_owned()
}

fn main() {
    println!(
        "cargo::rerun-if-changed={}",
        git_output(&["rev-parse", "--path-format=absolute", "--git-path", "HEAD"])
    );
    println!(
        "cargo::rerun-if-changed={}",
        git_output(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "packed-refs"
        ])
    );
    if let Ok(reference) = Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
    {
        if reference.status.success() {
            let reference = String::from_utf8(reference.stdout).expect("Git reference is UTF-8");
            println!(
                "cargo::rerun-if-changed={}",
                git_output(&[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    reference.trim()
                ])
            );
        }
    }

    println!(
        "cargo::rustc-env=WT_GIT_COMMIT_SHA={}",
        git_output(&["rev-parse", "HEAD"])
    );
}
