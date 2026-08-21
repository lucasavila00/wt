use super::*;
use std::sync::Mutex;

static PROMPT_LOCK: Mutex<()> = Mutex::new(());
use uuid::Uuid;
use wt_control_protocol::{
    Capacity, CapacityResource, Instance, InstanceApplication, InstanceName, InstanceStatus,
    SshAccess,
};

fn item(context: &str, name: &str, status: InstanceStatus) -> ContextInstance {
    ContextInstance {
        context: context.to_owned(),
        agent_git_report_count: 0,
        disk_usage_bytes: None,
        instance: Instance {
            id: Uuid::new_v4(),
            name: InstanceName::parse(name).unwrap(),
            owner: "tester".to_owned(),
            status,
            vcpus: 2,
            memory_mib: 4096,
            disk_gib: 32,
            guest_ip: None,
            last_error: None,
            ssh: None,
            application: InstanceApplication::Devcontainer {
                source: "git@example.test:repo.git".to_owned(),
                git_base: "main".to_owned(),
                git_prefix: format!("{name}/"),
                app_ssh: None,
            },
        },
    }
}

#[test]
fn formats_aligned_instance_columns_without_tabs() {
    let provisioning = item("local", "jsdev-manual", InstanceStatus::Provisioning);
    let mut running = item("remote-lab", "a", InstanceStatus::Running);
    running.instance.memory_mib = 1536;
    running.instance.guest_ip = Some("192.0.2.10".to_owned());
    running.instance.ssh = Some(SshAccess {
        user: "wt".to_owned(),
        host: "192.0.2.10".to_owned(),
        port: 2222,
        host_keys: Vec::new(),
    });

    let output = format_instances(&[provisioning, running]);

    insta::assert_snapshot!(output, @r###"
    CONTEXT     NAME          KIND          STATUS        REPO  RESOURCES              DETAIL
    local       jsdev-manual  devcontainer  provisioning  repo  2 CPU · 4G · 32G       -
    remote-lab  a             devcontainer  running       repo  2 CPU · 1536MiB · 32G  -
    "###);
    assert!(!output.contains('\t'));
}

#[test]
fn formats_header_for_empty_inventory() {
    insta::assert_snapshot!(format_instances(&[]), @"CONTEXT  NAME  KIND  STATUS  REPO  RESOURCES  DETAIL");
}

#[test]
fn formats_reconciliation_error_details() {
    let mut failed = item("local", "jsdev", InstanceStatus::Error);
    failed.instance.last_error = Some("SSH endpoint identity mismatch".to_owned());

    insta::assert_snapshot!(format_instances(&[failed]), @r###"
    CONTEXT  NAME   KIND          STATUS  REPO  RESOURCES         DETAIL
    local    jsdev  devcontainer  error   repo  2 CPU · 4G · 32G  SSH endpoint identity mismatch; run `wt rm local.jsdev`
    "###);
}

#[test]
fn formats_stopped_world_with_recovery_commands() {
    let mut stopped = item("ars", "mt3", InstanceStatus::Stopped);
    stopped.instance.last_error = Some("guest stopped (crashed)".to_owned());
    stopped.disk_usage_bytes = Some(1536 * 1024 * 1024);

    insta::assert_snapshot!(format_instances(&[stopped]), @r###"
    CONTEXT  NAME  KIND          STATUS   REPO  RESOURCES               DETAIL
    ars      mt3   devcontainer  stopped  repo  2 CPU · 4G · 1.5G disk  guest stopped (crashed); run `wt start ars.mt3` or `wt rm ars.mt3`
    "###);
}

#[test]
fn ls_points_to_ag_git_reports_without_changing_world_status() {
    let mut running = item("local", "jsdev", InstanceStatus::Running);
    running.agent_git_report_count = 2;

    insta::assert_snapshot!(format_instances(&[running]), @r###"
    CONTEXT  NAME   KIND          STATUS   REPO  RESOURCES         DETAIL
    local    jsdev  devcontainer  running  repo  2 CPU · 4G · 32G  2 ag-git reports; run `wt reports`
    "###);
}

#[test]
fn explains_memory_capacity() {
    insta::assert_snapshot!(
        capacity_message(
            "ars",
            &wt_control_protocol::InstanceName::parse("mt3").unwrap(),
            &Capacity {
                resource: CapacityResource::Memory,
                total: 32_000,
                reserved: 32_000,
                requested: 8_000,
            },
        ),
        @r###"
    ars has 32000 MiB of 32000 MiB world and runner memory reserved; mt3 requests 8000 MiB.
    Free capacity with `wt ls` and `wt stop CONTEXT.WORLD` or `wt rm CONTEXT.WORLD`.
    "###
    );
}

#[test]
fn parses_code_target() {
    let cli = Cli::try_parse_from(["wt", "code", "ars.jsdev"]).unwrap();
    let Command::Code { name } = cli.command else {
        panic!("expected code command");
    };
    assert_eq!(name, "ars.jsdev");
}

#[test]
fn parses_ssh_target() {
    let cli = Cli::try_parse_from(["wt", "ssh", "ars.jsdev"]).unwrap();
    let Command::Ssh { name } = cli.command else {
        panic!("expected ssh command");
    };
    assert_eq!(name, "ars.jsdev");
}

#[test]
fn parses_start_target() {
    let cli = Cli::try_parse_from(["wt", "start", "ars.mt3"]).unwrap();
    let Command::Start { name } = cli.command else {
        panic!("expected start command");
    };
    assert_eq!(name, "ars.mt3");
}

#[test]
fn parses_stop_target() {
    let cli = Cli::try_parse_from(["wt", "stop", "ars.mt3"]).unwrap();
    let Command::Stop { name } = cli.command else {
        panic!("expected stop command");
    };
    assert_eq!(name, "ars.mt3");
}

#[test]
fn parses_agent_git_report_commands() {
    assert!(matches!(
        Cli::try_parse_from(["wt", "reports"]).unwrap().command,
        Command::Reports
    ));
    assert!(matches!(
        Cli::try_parse_from(["wt", "clear-reports"])
            .unwrap()
            .command,
        Command::ClearReports
    ));
}

#[test]
fn new_is_interactive_only() {
    assert!(matches!(
        Cli::try_parse_from(["wt", "new"]).unwrap().command,
        Command::New { kind: None }
    ));
    assert!(Cli::try_parse_from(["wt", "new", "git@example.test:repo.git"]).is_err());
}

#[test]
fn parses_host_name_with_default_recipe() {
    let cli = Cli::try_parse_from(["wt", "new", "host", "sandbox"]).unwrap();
    let Command::New {
        kind: Some(host::NewKind::Host(input)),
    } = cli.command
    else {
        panic!("expected host new command")
    };
    assert_eq!(input.name, InstanceName::parse("sandbox").unwrap());
    assert_eq!(input.user_data, None);
}

#[test]
fn parses_host_recipe_override() {
    let cli = Cli::try_parse_from(["wt", "new", "host", "sandbox", "--user-data", "recipe.yaml"])
        .unwrap();
    let Command::New {
        kind: Some(host::NewKind::Host(input)),
    } = cli.command
    else {
        panic!("expected host new command")
    };
    assert_eq!(input.name, InstanceName::parse("sandbox").unwrap());
    assert_eq!(input.user_data, Some(PathBuf::from("recipe.yaml")));
}

#[test]
fn prompt_cancels_after_a_signal() {
    let _lock = PROMPT_LOCK.lock().unwrap();
    CANCELLED.store(false, Ordering::SeqCst);
    cancel_prompt(0);
    let error = prompt_error(std::io::Error::other("prompt failed"));
    assert_eq!(error.to_string(), "creation cancelled");
    CANCELLED.store(false, Ordering::SeqCst);
}

#[test]
fn parses_git_author_values_without_losing_spaces_or_unicode() {
    assert_eq!(
        parse_git_config_value("Lucas Ávila \0".as_bytes()).unwrap(),
        Some("Lucas Ávila ".to_owned())
    );
    assert_eq!(parse_git_config_value(b"\0").unwrap(), None);
}

#[test]
fn explains_required_git_author_value() {
    insta::assert_snapshot!(
        required_git_config_error("user.email", None).to_string(),
        @"global Git user.email is required; configure it with `git config --global user.email VALUE`"
    );
}
