use crate::runner::{args, Runner};
use anyhow::{bail, Context, Result};
use nix::unistd::{getgroups, Gid, Group, Uid, User};
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use wt_libvirt::{ServerConfig, LIBVIRT_URI};

pub(crate) fn preflight(runner: &impl Runner) -> Result<()> {
    require_host_platform()?;
    require_kvm()?;
    require_active_group("kvm")?;
    require_active_group("libvirt")?;
    require_active_group("docker")?;
    require_libvirt_qemu_identity()?;
    runner.text(
        "virsh",
        &args(["-c", LIBVIRT_URI, "domcapabilities", "--virttype", "kvm"]),
        "verify libvirt KVM capability",
    )?;
    Ok(())
}

pub(crate) fn prepare_state(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    ensure_network(runner, &config.libvirt.network)?;
    ensure_directories(runner, config)
}

fn require_libvirt_qemu_identity() -> Result<()> {
    let kvm = Group::from_name("kvm")
        .context("look up kvm group")?
        .context("required group does not exist: kvm")?;
    let qemu = User::from_name("libvirt-qemu")
        .context("look up libvirt-qemu user")?
        .context("required user does not exist: libvirt-qemu")?;
    if qemu.gid != kvm.gid {
        bail!("libvirt-qemu must use kvm as its primary group");
    }
    Ok(())
}

fn require_host_platform() -> Result<()> {
    if std::env::consts::ARCH != "x86_64" {
        bail!("Ubuntu 24.04 amd64 is required");
    }
    let release = fs::read_to_string("/etc/os-release").context("read /etc/os-release")?;
    let id = os_release_value(&release, "ID");
    let version = os_release_value(&release, "VERSION_ID");
    if id.as_deref() != Some("ubuntu") || version.as_deref() != Some("24.04") {
        bail!("Ubuntu 24.04 amd64 is required");
    }
    Ok(())
}

fn os_release_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (actual_key, value) = line.split_once('=')?;
        (actual_key == key).then(|| value.trim_matches('"').to_owned())
    })
}

fn require_kvm() -> Result<()> {
    let metadata = fs::metadata("/dev/kvm").context("KVM is required: read /dev/kvm")?;
    if !metadata.file_type().is_char_device() {
        bail!("KVM is required: /dev/kvm is not a character device");
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .context("KVM is required: open /dev/kvm for read/write")?;
    Ok(())
}

fn require_active_group(name: &str) -> Result<()> {
    let group = Group::from_name(name)
        .with_context(|| format!("look up group {name}"))?
        .with_context(|| format!("required group does not exist: {name}"))?;
    let active = group.gid == Gid::effective()
        || getgroups()
            .context("read active process groups")?
            .contains(&group.gid);
    if !active {
        bail!("group {name} is not active; log out, log back in, and rerun");
    }
    Ok(())
}

fn ensure_network(runner: &impl Runner, network: &str) -> Result<()> {
    let info = runner.text(
        "virsh",
        &args(["-c", LIBVIRT_URI, "net-info", network]),
        "inspect configured libvirt network",
    )?;
    if !info.lines().any(|line| field_is(line, "Active", "yes")) {
        runner.run(
            "virsh",
            &args(["-c", LIBVIRT_URI, "net-start", network]),
            "start configured libvirt network",
        )?;
    }
    if !info.lines().any(|line| field_is(line, "Autostart", "yes")) {
        runner.run(
            "virsh",
            &args(["-c", LIBVIRT_URI, "net-autostart", network]),
            "enable configured libvirt network",
        )?;
    }
    Ok(())
}

fn field_is(line: &str, field: &str, value: &str) -> bool {
    line.split_once(':')
        .map(|(key, actual)| key.trim() == field && actual.trim() == value)
        .unwrap_or(false)
}

fn ensure_directories(runner: &impl Runner, config: &ServerConfig) -> Result<()> {
    let image_dir = config
        .image
        .installed_path
        .parent()
        .context("image.installed_path has no parent")?;
    ensure_directory(runner, image_dir, Uid::from_raw(0), Gid::from_raw(0), 0o755)?;
    ensure_directory(
        runner,
        &config.install.binary_dir,
        Uid::from_raw(0),
        Gid::from_raw(0),
        0o755,
    )?;

    let kvm_gid = Group::from_name("kvm")
        .context("look up kvm group")?
        .context("required group does not exist: kvm")?
        .gid;
    ensure_directory(
        runner,
        &config.libvirt.worlds_dir,
        Uid::effective(),
        kvm_gid,
        0o2770,
    )?;
    ensure_directory(
        runner,
        &config.registry_cache.state_dir,
        Uid::from_raw(0),
        Gid::from_raw(0),
        0o755,
    )?;
    ensure_qemu_search_acl(runner, &config.libvirt.worlds_dir)
}

pub(crate) fn ensure_qemu_search_acl(runner: &impl Runner, path: &Path) -> Result<()> {
    let output = runner.text(
        "getfacl",
        &["-cp".into(), "--".into(), path.as_os_str().to_owned()],
        "inspect libvirt-qemu directory ACL",
    )?;
    let entries = acl_entries(&output);
    let expected =
        acl_entries("user::rwx\nuser:libvirt-qemu:--x\ngroup::rwx\nmask::rwx\nother::---\n");
    if entries == expected {
        return Ok(());
    }
    let legacy = acl_entries("user::rwx\ngroup::rwx\nother::---\n");
    if entries != legacy {
        bail!(
            "directory ACL drift at {}: expected only user:libvirt-qemu:--x in addition to mode 2770",
            path.display()
        );
    }
    runner.run(
        "sudo",
        &[
            "setfacl".into(),
            "-m".into(),
            "u:libvirt-qemu:--x".into(),
            "--".into(),
            path.as_os_str().to_owned(),
        ],
        "grant libvirt-qemu search access to directory",
    )
}

fn acl_entries(contents: &str) -> Vec<String> {
    let mut entries = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn ensure_directory(
    runner: &impl Runner,
    path: &Path,
    uid: Uid,
    gid: Gid,
    mode: u32,
) -> Result<()> {
    if path.exists() {
        let metadata =
            fs::metadata(path).with_context(|| format!("inspect directory {}", path.display()))?;
        // Existing server state is evidence of the installed contract. Never silently repair drift.
        if !metadata.is_dir()
            || metadata.uid() != uid.as_raw()
            || metadata.gid() != gid.as_raw()
            || metadata.mode() & 0o7777 != mode
        {
            bail!(
                "directory drift at {}: expected uid={}, gid={}, mode={mode:04o}",
                path.display(),
                uid.as_raw(),
                gid.as_raw()
            );
        }
        return Ok(());
    }
    runner.run(
        "sudo",
        &[
            "install".into(),
            "-d".into(),
            "-o".into(),
            uid.as_raw().to_string().into(),
            "-g".into(),
            gid.as_raw().to_string().into(),
            "-m".into(),
            format!("{mode:04o}").into(),
            path.as_os_str().to_owned(),
        ],
        "create server directory",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::ffi::OsString;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Output;

    struct FakeRunner {
        outputs: RefCell<VecDeque<Output>>,
        calls: RefCell<Vec<(String, Vec<OsString>)>>,
    }

    impl FakeRunner {
        fn new(outputs: impl IntoIterator<Item = (&'static str, bool)>) -> Self {
            Self {
                outputs: RefCell::new(
                    outputs
                        .into_iter()
                        .map(|(stdout, success)| Output {
                            status: std::process::ExitStatus::from_raw(if success { 0 } else { 1 }),
                            stdout: stdout.as_bytes().to_vec(),
                            stderr: Vec::new(),
                        })
                        .collect(),
                ),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Runner for FakeRunner {
        fn output(&self, program: &str, args: &[OsString]) -> Result<Output> {
            self.calls
                .borrow_mut()
                .push((program.to_owned(), args.to_vec()));
            self.outputs
                .borrow_mut()
                .pop_front()
                .context("unexpected fake command")
        }
    }

    #[test]
    fn virsh_fields_are_parsed_exactly() {
        assert!(field_is("Active:         yes", "Active", "yes"));
        assert!(!field_is("Active:         no", "Active", "yes"));
        assert!(!field_is("Inactive:       yes", "Active", "yes"));
    }

    #[test]
    fn os_release_values_are_parsed_exactly() {
        let release = "ID=ubuntu\nVERSION_ID=\"24.04\"\n";
        assert_eq!(os_release_value(release, "ID").as_deref(), Some("ubuntu"));
        assert_eq!(
            os_release_value(release, "VERSION_ID").as_deref(),
            Some("24.04")
        );
    }

    #[test]
    fn matching_network_is_not_mutated() {
        let runner = FakeRunner::new([("Active: yes\nAutostart: yes\n", true)]);
        ensure_network(&runner, "default").unwrap();
        assert_eq!(runner.calls.borrow().len(), 1);
    }

    #[test]
    fn inactive_network_is_started_and_enabled() {
        let runner = FakeRunner::new([
            ("Active: no\nAutostart: no\n", true),
            ("", true),
            ("", true),
        ]);
        ensure_network(&runner, "server").unwrap();
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert!(calls[1].1.iter().any(|argument| argument == "net-start"));
        assert!(calls[2]
            .1
            .iter()
            .any(|argument| argument == "net-autostart"));
    }

    #[test]
    fn acl_entries_ignore_headers_and_order() {
        assert_eq!(
            acl_entries("# file: /tmp/worlds\nother::---\nuser::rwx\ngroup::rwx\n"),
            acl_entries("user::rwx\ngroup::rwx\nother::---\n")
        );
    }

    #[test]
    fn legacy_worlds_acl_is_upgraded() {
        let runner = FakeRunner::new([("user::rwx\ngroup::rwx\nother::---\n", true), ("", true)]);
        ensure_qemu_search_acl(&runner, Path::new("/worlds")).unwrap();
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].0, "sudo");
        assert!(calls[1]
            .1
            .iter()
            .any(|argument| argument == "u:libvirt-qemu:--x"));
    }

    #[test]
    fn unexpected_worlds_acl_is_drift() {
        let runner = FakeRunner::new([(
            "user::rwx\nuser:other:rwx\ngroup::rwx\nmask::rwx\nother::---\n",
            true,
        )]);
        assert!(ensure_qemu_search_acl(&runner, Path::new("/worlds")).is_err());
        assert_eq!(runner.calls.borrow().len(), 1);
    }
}
