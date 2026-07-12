//! Interactive tmux pane entrypoint for the primary devcontainer.

use serde::Deserialize;
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

const WORKSPACE: &str = "/workspace";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerInspect {
    mounts: Vec<Mount>,
    config: ContainerConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Mount {
    source: String,
    destination: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerConfig {
    #[serde(default)]
    labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DevcontainerMetadata {
    #[serde(rename = "containerUser")]
    container_user: Option<String>,
    #[serde(rename = "remoteUser")]
    remote_user: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct ShellTarget {
    container: String,
    workspace: String,
    user: Option<String>,
}

fn main() {
    let target = match shell_target() {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let error = command(&target).exec();
    eprintln!("wt: start the devcontainer shell: {error}");
    std::process::exit(1);
}

fn shell_target() -> Result<ShellTarget, String> {
    let containers = docker(&[
        "ps",
        "--filter",
        "label=devcontainer.local_folder=/workspace",
        "--format",
        "{{.ID}}",
    ])?;
    let container = select_container(&containers)?;
    let inspect = docker(&["inspect", &container])?;
    inspect_target(container, &inspect)
}

fn docker(args: &[&str]) -> Result<String, String> {
    let output = Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("wt: run docker: {error}"))?;
    if !output.status.success() {
        return Err(format!("wt: docker exited with {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("wt: read docker output: {error}"))
}

fn select_container(output: &str) -> Result<String, String> {
    let containers = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    match containers.as_slice() {
        [] => Err("wt: the devcontainer app is not running".to_owned()),
        [container] => Ok((*container).to_owned()),
        _ => Err("wt: multiple devcontainer app containers match /workspace".to_owned()),
    }
}

fn inspect_target(container: String, output: &str) -> Result<ShellTarget, String> {
    let mut containers: Vec<ContainerInspect> =
        serde_json::from_str(output).map_err(|error| format!("wt: inspect container: {error}"))?;
    if containers.len() != 1 {
        return Err("wt: docker inspect returned an unexpected number of containers".to_owned());
    }
    let inspected = containers.pop().expect("length checked");
    let workspace = inspected
        .mounts
        .into_iter()
        .find(|mount| mount.source == WORKSPACE)
        .map(|mount| mount.destination)
        .filter(|destination| !destination.is_empty())
        .ok_or_else(|| "wt: the devcontainer app does not mount /workspace".to_owned())?;
    let user = inspected
        .config
        .labels
        .get("devcontainer.metadata")
        .map(|metadata| metadata_user(metadata))
        .transpose()?
        .flatten();
    Ok(ShellTarget {
        container,
        workspace,
        user,
    })
}

fn metadata_user(metadata: &str) -> Result<Option<String>, String> {
    let entries: Vec<DevcontainerMetadata> = serde_json::from_str(metadata)
        .map_err(|error| format!("wt: read devcontainer metadata: {error}"))?;
    let mut container_user = None;
    let mut remote_user = None;
    for entry in entries {
        if let Some(value) = entry.container_user {
            container_user = Some(value);
        }
        if let Some(value) = entry.remote_user {
            remote_user = Some(value);
        }
    }
    Ok(remote_user
        .filter(|value| !value.is_empty())
        .or_else(|| container_user.filter(|value| !value.is_empty())))
}

fn command(target: &ShellTarget) -> Command {
    let mut command = Command::new("docker");
    command.args(["exec", "-it", "--workdir", &target.workspace]);
    if let Some(user) = &target.user {
        command.args(["--user", user]);
    }
    command.args([&target.container, "/bin/bash"]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_exactly_one_container() {
        assert_eq!(
            select_container("").unwrap_err(),
            "wt: the devcontainer app is not running"
        );
        assert_eq!(select_container("abc\n").unwrap(), "abc");
        assert_eq!(
            select_container("abc\ndef\n").unwrap_err(),
            "wt: multiple devcontainer app containers match /workspace"
        );
    }

    #[test]
    fn reads_workspace_and_last_metadata_values() {
        let output = r#"[{
            "Mounts": [{"Source":"/workspace","Destination":"/workspaces/project"}],
            "Config": {"Labels": {"devcontainer.metadata":"[{\"containerUser\":\"root\"},{\"containerUser\":\"node\",\"remoteUser\":\"vscode\"}]"}}
        }]"#;
        assert_eq!(
            inspect_target("abc".to_owned(), output).unwrap(),
            ShellTarget {
                container: "abc".to_owned(),
                workspace: "/workspaces/project".to_owned(),
                user: Some("vscode".to_owned()),
            }
        );
    }

    #[test]
    fn remote_user_falls_back_to_container_user() {
        assert_eq!(
            metadata_user(r#"[{"containerUser":"node","remoteUser":""}]"#).unwrap(),
            Some("node".to_owned())
        );
        assert_eq!(metadata_user("[]").unwrap(), None);
    }

    #[test]
    fn builds_docker_exec_arguments() {
        let target = ShellTarget {
            container: "abc".to_owned(),
            workspace: "/workspaces/project".to_owned(),
            user: Some("vscode".to_owned()),
        };
        let command = command(&target);
        assert_eq!(command.get_program(), "docker");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "exec",
                "-it",
                "--workdir",
                "/workspaces/project",
                "--user",
                "vscode",
                "abc",
                "/bin/bash",
            ]
        );
    }
}
