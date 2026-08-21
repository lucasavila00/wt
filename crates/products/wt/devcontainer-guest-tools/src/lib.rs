use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::process::{Command, ExitStatus, Stdio};

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

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
struct ConfigurationOutput {
    configuration: DevcontainerConfiguration,
}

#[derive(Debug, Deserialize)]
struct DevcontainerConfiguration {
    #[serde(rename = "remoteUser")]
    remote_user: Option<String>,
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
    #[serde(rename = "remoteUser")]
    remote_user: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DevcontainerMetadataLabel {
    One(DevcontainerMetadata),
    Many(Vec<DevcontainerMetadata>),
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

pub fn configured_remote_user(output: &str) -> Result<String, String> {
    let output: ConfigurationOutput = serde_json::from_str(output)
        .map_err(|error| format!("wt: read devcontainer configuration: {error}"))?;
    let user = output
        .configuration
        .remote_user
        .ok_or_else(|| "wt: devcontainer configuration must set remoteUser".to_owned())?;
    validate_user(&user, "devcontainer configuration")?;
    Ok(user)
}

pub fn verify_app_user(expected: &str) -> Result<(), String> {
    validate_user(expected, "devcontainer configuration")?;
    let target = app_target()?;
    verify_runtime_user(expected, &target.user)?;
    let output = cmd!("docker")
        .args([
            "exec",
            "--user",
            expected,
            &target.container,
            "/usr/bin/id",
            "-un",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("wt: verify devcontainer remoteUser {expected:?}: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "wt: devcontainer remoteUser {expected:?} is not an existing container account: {}",
            detail.trim()
        ));
    }
    let resolved = String::from_utf8(output.stdout)
        .map_err(|error| format!("wt: read devcontainer remoteUser: {error}"))?;
    if resolved.trim() != expected {
        return Err(format!(
            "wt: devcontainer remoteUser {expected:?} resolves to a different account {:?}",
            resolved.trim()
        ));
    }
    Ok(())
}

fn verify_runtime_user(expected: &str, actual: &str) -> Result<(), String> {
    if actual != expected {
        return Err(format!(
            "wt: devcontainer remoteUser changed from {expected:?} to {actual:?} at runtime"
        ));
    }
    Ok(())
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
        .ok_or_else(|| "wt: devcontainer runtime metadata has no remoteUser".to_owned())
        .and_then(|metadata| metadata_user(metadata))?;
    validate_user(&user, "devcontainer runtime metadata")?;
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

fn metadata_user(metadata: &str) -> Result<String, String> {
    let entries = serde_json::from_str(metadata)
        .map_err(|error| format!("wt: read devcontainer metadata: {error}"))?;
    let entries = match entries {
        DevcontainerMetadataLabel::One(entry) => vec![entry],
        DevcontainerMetadataLabel::Many(entries) => entries,
    };
    let mut remote_user = None;
    for entry in entries {
        if let Some(value) = entry.remote_user {
            remote_user = Some(value);
        }
    }
    remote_user.ok_or_else(|| "wt: devcontainer runtime metadata has no remoteUser".to_owned())
}

fn validate_user(user: &str, source: &str) -> Result<(), String> {
    if user.is_empty()
        || !user
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("wt: {source} has invalid remoteUser {user:?}"));
    }
    Ok(())
}

pub fn pane_command(target: &AppTarget, pane_id: Option<&str>) -> Result<Command, String> {
    let remote = format!("{}@{}", target.user, target.address);
    let pane_environment = match pane_id {
        Some(pane_id) if valid_pane_id(pane_id) => format!(
            "export WT_BYOBU_SESSION=wt-app WT_BYOBU_PANE={}; ",
            shell_quote(pane_id)
        ),
        Some(_) => return Err("wt: invalid TMUX_PANE for devcontainer shell".to_owned()),
        None => String::new(),
    };
    let command = format!(
        "{pane_environment}cd -- {} && exec /bin/bash -l",
        shell_quote(&target.workspace),
    );
    Ok(cmd!(
        "/usr/bin/ssh",
        "-tt",
        "-A",
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
    ))
}

fn valid_pane_id(value: &str) -> bool {
    value.strip_prefix('%').is_some_and(|number| {
        !number.is_empty() && number.len() <= 16 && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub fn pane_failure_diagnostic(status: &ExitStatus) -> Option<String> {
    if status.success() {
        return None;
    }
    let status = status
        .code()
        .map(|code| format!("exit {code}"))
        .unwrap_or_else(|| status.to_string());
    Some(format!(
        "wt: could not open the devcontainer shell ({status})\n\
         wt: fix the error above, close this pane, and create a new one"
    ))
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
    fn reads_object_devcontainer_metadata() {
        assert_eq!(
            metadata_user(r#"{"containerUser":"node","remoteUser":"vscode"}"#).unwrap(),
            "vscode"
        );
    }

    #[test]
    fn requires_configured_remote_user() {
        assert_eq!(
            configured_remote_user(r#"{"configuration":{"remoteUser":"vscode"}}"#).unwrap(),
            "vscode"
        );
        insta::assert_snapshot!(
            configured_remote_user(r#"{"configuration":{}}"#).unwrap_err(),
            @"wt: devcontainer configuration must set remoteUser"
        );
        insta::assert_snapshot!(
            configured_remote_user(r#"{"configuration":{"remoteUser":""}}"#).unwrap_err(),
            @r###"wt: devcontainer configuration has invalid remoteUser """###
        );
        insta::assert_snapshot!(
            configured_remote_user(r#"{"configuration":{"remoteUser":0}}"#).unwrap_err(),
            @r###"wt: read devcontainer configuration: invalid type: integer `0`, expected a string at line 1 column 32"###
        );
    }

    #[test]
    fn runtime_metadata_does_not_fall_back_to_container_user() {
        insta::assert_snapshot!(
            metadata_user(r#"{"containerUser":"node"}"#).unwrap_err(),
            @"wt: devcontainer runtime metadata has no remoteUser"
        );
    }

    #[test]
    fn runtime_user_must_match_configuration() {
        insta::assert_snapshot!(
            verify_runtime_user("vscode", "root").unwrap_err(),
            @r###"wt: devcontainer remoteUser changed from "vscode" to "root" at runtime"###
        );
    }

    #[test]
    fn pane_uses_ssh_instead_of_docker_exec() {
        let target = inspect_target("abc".to_owned(), INSPECT).unwrap();
        let command = pane_command(&target, Some("%12")).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/ssh");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg == "-A"));
        assert!(args.iter().any(|arg| arg == "vscode@172.18.0.3"));
        assert!(args.iter().any(|arg| arg.contains("WT_BYOBU_PANE='%12'")));
        assert!(!args.iter().any(|arg| arg.contains("docker")));
    }

    #[test]
    fn pane_rejects_invalid_tmux_identity() {
        let target = inspect_target("abc".to_owned(), INSPECT).unwrap();
        assert_eq!(
            pane_command(&target, Some("other")).unwrap_err(),
            "wt: invalid TMUX_PANE for devcontainer shell"
        );
    }

    #[test]
    fn pane_failure_diagnostic_explains_recovery() {
        let failure = Command::new("/bin/sh")
            .args(["-c", "exit 127"])
            .status()
            .unwrap();
        insta::assert_snapshot!(pane_failure_diagnostic(&failure).unwrap(), @r###"
        wt: could not open the devcontainer shell (exit 127)
        wt: fix the error above, close this pane, and create a new one
        "###);

        let success = Command::new("/bin/true").status().unwrap();
        assert_eq!(pane_failure_diagnostic(&success), None);
    }
}
