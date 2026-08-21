use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const SNAPSHOT_LINE_LIMIT: usize = 1_000;
const EXCEPTIONS: &str = "crates/tests/repository-checks/snapshot-line-limit-exceptions";

fn main() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let [command] = arguments.as_slice() else {
        bail!("usage: wt-repository-checks snapshot-lines");
    };
    match command.as_str() {
        "snapshot-lines" => check_snapshot_lines(),
        _ => bail!("unknown repository check {command:?}"),
    }
}

fn check_snapshot_lines() -> Result<()> {
    let exceptions = read_exceptions()?;
    let mut paths = Vec::new();
    collect_paths(Path::new("."), &mut paths)?;
    paths.sort();

    let mut failures = Vec::new();
    for path in paths {
        let relative = path.strip_prefix("./").unwrap_or(&path);
        if exceptions.contains(relative) {
            continue;
        }
        check_snapshot(relative, &mut failures)?;
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(failures.join("\n"))
    }
}

fn read_exceptions() -> Result<HashSet<PathBuf>> {
    let contents = fs::read_to_string(EXCEPTIONS)
        .with_context(|| format!("read snapshot exceptions from {EXCEPTIONS}"))?;
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .collect())
}

fn collect_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("read repository directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | "target")
            ) {
                collect_paths(&path, paths)?;
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("snap") {
            paths.push(path);
        }
    }
    Ok(())
}

fn check_snapshot(path: &Path, failures: &mut Vec<String>) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("read snapshot {}", path.display()))?;
    let lines = contents.lines().count();
    if lines > SNAPSHOT_LINE_LIMIT {
        failures.push(format!(
            "{} has {lines} lines (maximum {SNAPSHOT_LINE_LIMIT})",
            path.display()
        ));
    }
    Ok(())
}
