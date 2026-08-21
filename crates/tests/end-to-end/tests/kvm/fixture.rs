use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use wt_end_to_end_tests::cmd;

pub(crate) struct GitFixture {
    pub(crate) repository: PathBuf,
    pub(crate) guest_key: PathBuf,
    pub(crate) guest_public_key: PathBuf,
}

impl GitFixture {
    pub(crate) fn create(root: &Path) -> Self {
        let seed = root.join("project");
        fs::create_dir(&seed).unwrap();
        run(
            cmd!("git", "init", "-b", "main", &seed),
            "initialize fixture repository",
        );
        fs::write(seed.join("README.md"), "WT agent tool fixture\n").unwrap();
        run(
            cmd!("git", "-C", &seed, "config", "user.name", "WT E2E"),
            "configure fixture Git name",
        );
        run(
            cmd!(
                "git",
                "-C",
                &seed,
                "config",
                "user.email",
                "wt@example.invalid"
            ),
            "configure fixture Git email",
        );
        run(
            cmd!("git", "-C", &seed, "add", "."),
            "stage fixture repository",
        );
        run(
            cmd!("git", "-C", &seed, "commit", "-m", "fixture"),
            "commit fixture repository",
        );
        let repository = root.join("acme/widget.git");
        fs::create_dir_all(repository.parent().unwrap()).unwrap();
        run(
            cmd!("git", "clone", "--bare", &seed, &repository),
            "create bare fixture repository",
        );
        let guest_key = root.join("guest-client");
        generate_key(&guest_key, "");
        let guest_public_key = guest_key.with_extension("pub");
        Self {
            repository,
            guest_key,
            guest_public_key,
        }
    }

}

pub(crate) fn generate_key(path: &Path, passphrase: &str) {
    run(
        cmd!(
            "ssh-keygen",
            "-q",
            "-t",
            "ed25519",
            "-N",
            passphrase,
            "-f",
            path,
        ),
        "generate test SSH key",
    );
}

pub(crate) fn run(mut command: Command, action: &str) {
    let output = command.output().unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn ensure_success(action: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{action} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
