use super::{guest, WorldProvisioner};
use std::io::Write;
use std::time::Instant;
use wt_provider::{GuestTransport, Machine, SharedFolderMount, WorkerError};

const MOUNT_SHARED_FOLDERS: &[u8] =
    include_bytes!("../../../../assets/world/shared/mount-folders.sh");

impl WorldProvisioner {
    pub(crate) fn mount_shared_folders(
        &self,
        transport: &dyn GuestTransport,
        deadline: Instant,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        if self.config.shared_folders.is_empty() {
            return Ok(());
        }
        let args = shared_folder_args(&self.config.shared_folders)?;
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        guest::run_script(
            transport,
            "shared folder mounts",
            MOUNT_SHARED_FOLDERS,
            &args,
            deadline,
            log,
        )
    }

    pub(crate) fn mount_shared_folders_for(
        &self,
        machine: &Machine,
        log: &mut dyn Write,
    ) -> Result<(), WorkerError> {
        self.mount_shared_folders(
            machine.transport.as_ref(),
            Instant::now() + self.config.recipe_timeout,
            log,
        )
    }
}

fn shared_folder_args(folders: &[SharedFolderMount]) -> Result<Vec<String>, WorkerError> {
    let mut args = Vec::with_capacity(folders.len() * 2);
    for folder in folders {
        let target = folder.target.to_str().ok_or_else(|| {
            WorkerError::new(format!(
                "shared folder target is not UTF-8: {}",
                folder.target.display()
            ))
        })?;
        args.push(folder.tag.clone());
        args.push(target.to_owned());
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    #[test]
    fn mount_arguments_preserve_tags_and_targets() {
        insta::assert_debug_snapshot!(shared_folder_args(&[
            SharedFolderMount {
                tag: "wt-shared-0".to_owned(),
                target: PathBuf::from(".codex/sessions"),
            },
            SharedFolderMount {
                tag: "wt-shared-1".to_owned(),
                target: PathBuf::from(".claude/projects"),
            },
        ])
        .unwrap(), @r###"
        [
            "wt-shared-0",
            ".codex/sessions",
            "wt-shared-1",
            ".claude/projects",
        ]
        "###);
    }

    #[test]
    fn mount_script_reports_invalid_inputs() {
        fn failure(args: &[&str]) -> String {
            let mut child = Command::new("/bin/sh")
                .args(["-s", "--"])
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(MOUNT_SHARED_FOLDERS)
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(!output.status.success());
            String::from_utf8(output.stderr).unwrap()
        }

        insta::assert_snapshot!(failure(&[]), @"usage: mount-folders.sh TAG TARGET [TAG TARGET ...]\n");
        insta::assert_snapshot!(failure(&["not-a-tag", ".codex/sessions"]), @"invalid shared folder tag: not-a-tag\n");
        insta::assert_snapshot!(failure(&["wt-shared-0", "../sessions"]), @"invalid shared folder target: ../sessions\n");
    }
}
