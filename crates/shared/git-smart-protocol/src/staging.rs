//! Validate objects in a disposable repository before sending any upstream ref update.
use crate::packet::{packet_lines, push_commands, read_packet, read_packet_section, write_packet};
use crate::transport::spawn_git;
use crate::{GitService, GitTarget};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(repository: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(repository);
    command.env("GIT_NO_REPLACE_OBJECTS", "1");
    command
}

fn run(command: &mut Command) -> Result<()> {
    let output = command.output().context("run staging Git command")?;
    if !output.status.success() {
        bail!(
            "staging Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn index_pack(repository: &Path, input: Stdio) -> Result<Process> {
    Ok(Process(
        git(repository)
            .args(["index-pack", "--stdin", "--fix-thin", "--strict"])
            .stdin(input)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("start Git object validation")?,
    ))
}

// Fetch every advertised object: the client's thin pack can use delta bases
// reachable from any advertised ref, not only the branches being updated.
fn fetch_objects(repository: &Path, target: &GitTarget<'_>, format: &str) -> Result<()> {
    let mut upload = Process(spawn_git(target, GitService::UploadPack)?);
    let stderr = upload.0.stderr.take().context("upload-pack stderr")?;
    let stderr_reader =
        std::thread::spawn(move || std::io::copy(&mut { stderr }, &mut std::io::sink()));
    let result = (|| {
        let stdout = upload.0.stdout.as_mut().context("upload-pack stdout")?;
        let advertisement = read_packet_section(stdout)?;
        let objects: BTreeSet<_> = packet_lines(&advertisement)?
            .filter_map(|line| line.split(|byte| *byte == b' ').next())
            .filter(|oid| matches!(oid.len(), 40 | 64) && oid.iter().any(|byte| *byte != b'0'))
            .map(|oid| String::from_utf8(oid.to_vec()))
            .collect::<std::result::Result<_, _>>()?;
        if objects.is_empty() {
            return Ok(());
        }
        let mut stdin = upload.0.stdin.take().context("upload-pack stdin")?;
        for (index, oid) in objects.iter().enumerate() {
            let capability = if index == 0 && format == "sha256" {
                " object-format=sha256"
            } else {
                ""
            };
            write_packet(&mut stdin, format!("want {oid}{capability}\n").as_bytes())?;
        }
        stdin.write_all(b"0000")?;
        write_packet(&mut stdin, b"done\n")?;
        drop(stdin);
        let mut stdout = upload.0.stdout.take().context("upload-pack stdout")?;
        if read_packet(&mut stdout)? != b"0008NAK\n" {
            bail!("unexpected upstream fetch response");
        }
        let mut index = index_pack(repository, Stdio::from(stdout))?;
        if !index.0.wait()?.success() || !upload.0.wait()?.success() {
            bail!("could not fetch upstream objects for history validation");
        }
        Ok(())
    })();
    drop(upload);
    stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("upload-pack stderr reader panicked"))??;
    result
}

pub(crate) fn validated_pack(
    stream: &mut impl Read,
    target: &GitTarget<'_>,
    commands: &[u8],
) -> Result<File> {
    // These extensions carry additional input after/beside the pack. Fail closed
    // until the gateway can validate and forward their framing explicitly.
    if packet_lines(commands)?.any(|line| {
        line.split(|b| b.is_ascii_whitespace() || *b == 0)
            .any(|word| word == b"push-options" || word == b"push-cert")
    }) {
        bail!("push options and signed pushes are not supported by history validation");
    }
    let updates = push_commands(commands)?;
    let format = if updates[0].new_oid.len() == 64 {
        "sha256"
    } else {
        "sha1"
    };
    let temporary = tempfile::tempdir().context("create push staging directory")?;
    let repository = temporary.path();
    run(git(repository).args([
        "init",
        "--bare",
        "--template=",
        &format!("--object-format={format}"),
    ]))?;
    fetch_objects(repository, target, format)?;

    let mut incoming = tempfile::tempfile().context("create incoming pack")?;
    crate::pack::copy_pack(stream, &mut incoming, updates[0].new_oid.len() / 2)?;
    incoming.rewind()?;
    let mut index = index_pack(repository, Stdio::from(incoming.try_clone()?))?;
    if !index.0.wait()?.success() {
        bail!("invalid or incomplete Git pack");
    }
    for update in &updates {
        let kind = git(repository)
            .args(["cat-file", "-t", &update.new_oid])
            .output()?;
        if !kind.status.success() || kind.stdout != b"commit\n" {
            bail!("branch `{}` must point to a commit", update.reference);
        }
        if update.previous_oid.bytes().any(|byte| byte != b'0') {
            let status = git(repository)
                .args([
                    "merge-base",
                    "--is-ancestor",
                    &update.previous_oid,
                    &update.new_oid,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                bail!(
                    "non-fast-forward update to `{}` rejected; gateway preserves history",
                    update.reference
                );
            }
        }
    }

    // Replay exactly the validated pack, without any trailing client input.
    // Original old IDs still go upstream, preserving its concurrent-update check.
    incoming.rewind()?;
    Ok(incoming)
}

#[cfg(test)]
mod tests;
