use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

#[test]
#[ignore = "requires local libvirt/KVM and prepared image"]
fn local_cli_manages_docker_ready_kvm_guest() {
    let root = workspace_root();
    let wt = root.join("target/debug/wt");
    let helper = root.join("target/debug/wt-local");
    assert!(wt.is_file(), "build workspace binaries first");
    assert!(helper.is_file(), "build workspace binaries first");

    let temp = TempDir::new().unwrap();
    let name = format!(
        "era1-kvm-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let created = wt_command(&wt, &helper, temp.path())
        .args(["new", "example.invalid/repo", &name])
        .output()
        .unwrap();
    if !created.status.success() {
        panic!("wt new failed: {}", output_text(&created));
    }

    let result = (|| {
        let listed = wt_command(&wt, &helper, temp.path())
            .arg("ls")
            .output()
            .map_err(|error| error.to_string())?;
        ensure_success("wt ls", &listed)?;
        if !String::from_utf8_lossy(&listed.stdout).contains(&name) {
            return Err("wt ls did not contain the created world".to_owned());
        }

        Ok(())
    })();

    let removed = wt_command(&wt, &helper, temp.path())
        .args(["rm", &name])
        .output()
        .unwrap();
    ensure_success("wt rm", &removed).unwrap();
    result.unwrap();
}

fn wt_command(wt: &Path, helper: &Path, state_root: &Path) -> Command {
    let mut command = Command::new(wt);
    command
        .env("WT_HELPER", helper)
        .env("WT_STATE_DIR", state_root.join("state"));
    command
}

fn ensure_success(action: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{action} failed: {}", output_text(output)))
    }
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
