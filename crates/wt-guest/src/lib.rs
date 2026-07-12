use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::process::{Command, Stdio};
use wt_command::cmd;

pub const APP_SSH_PORT: u16 = 2222;
pub const SESSION_IDENTITY: &str = "/var/lib/wt-app-ssh/session_identity";
pub const SESSION_KNOWN_HOSTS: &str = "/var/lib/wt-app-ssh/known_hosts";
const WORKSPACE: &str = "/workspace";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerInspect {
    mounts: Vec<Mount>,
    config: ContainerConfig,
    network_settings: NetworkSettings,
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
#[serde(rename_all = "PascalCase")]
struct NetworkSettings {
    networks: BTreeMap<String, ContainerNetwork>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerNetwork {
    #[serde(rename = "IPAddress")]
    ip_address: String,
}

#[derive(Debug, Deserialize)]
struct DevcontainerMetadata {
    #[serde(rename = "containerUser")]
    container_user: Option<String>,
    #[serde(rename = "remoteUser")]
    remote_user: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AppTarget {
    pub container: String,
    pub workspace: String,
    pub user: String,
    pub address: String,
}

pub fn app_target() -> Result<AppTarget, String> {
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
    let output = cmd!("docker")
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

fn inspect_target(container: String, output: &str) -> Result<AppTarget, String> {
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
        .flatten()
        .unwrap_or_else(|| "root".to_owned());
    if !user
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err("wt: the devcontainer app has an invalid remote user".to_owned());
    }
    let address = inspected
        .network_settings
        .networks
        .into_values()
        .map(|network| network.ip_address)
        .find(|address| !address.is_empty())
        .ok_or_else(|| "wt: the devcontainer app has no network address".to_owned())?;
    Ok(AppTarget {
        container,
        workspace,
        user,
        address,
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

pub fn pane_command(target: &AppTarget) -> Command {
    let remote = format!("{}@{}", target.user, target.address);
    let command = format!(
        "cd -- {} && exec /bin/bash -l",
        shell_quote(&target.workspace)
    );
    cmd!(
        "/usr/bin/ssh",
        "-tt",
        "-p",
        APP_SSH_PORT.to_string(),
        "-i",
        SESSION_IDENTITY,
        "-o",
        "BatchMode=yes",
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        format!("UserKnownHostsFile={SESSION_KNOWN_HOSTS}"),
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "HostKeyAlias=wt-app",
        "-o",
        "LogLevel=ERROR",
        remote,
        command,
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSPECT: &str = r#"[{
        "Mounts": [{"Source":"/workspace","Destination":"/workspaces/project"}],
        "Config": {"Labels": {"devcontainer.metadata":"[{\"containerUser\":\"root\"},{\"containerUser\":\"node\",\"remoteUser\":\"vscode\"}]"}},
        "NetworkSettings": {"Networks": {"project_default":{"IPAddress":"172.18.0.3"}}}
    }]"#;

    #[test]
    fn requires_exactly_one_container() {
        assert_eq!(
            select_container("").unwrap_err(),
            "wt: the devcontainer app is not running"
        );
        assert_eq!(select_container("abc\n").unwrap(), "abc");
        assert!(select_container("abc\ndef\n").is_err());
    }

    #[test]
    fn reads_workspace_user_and_network_address() {
        assert_eq!(
            inspect_target("abc".to_owned(), INSPECT).unwrap(),
            AppTarget {
                container: "abc".to_owned(),
                workspace: "/workspaces/project".to_owned(),
                user: "vscode".to_owned(),
                address: "172.18.0.3".to_owned(),
            }
        );
    }

    #[test]
    fn pane_uses_ssh_instead_of_docker_exec() {
        let target = inspect_target("abc".to_owned(), INSPECT).unwrap();
        let command = pane_command(&target);
        assert_eq!(command.get_program(), "/usr/bin/ssh");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg == "vscode@172.18.0.3"));
        assert!(!args.iter().any(|arg| arg.contains("docker")));
    }
}
