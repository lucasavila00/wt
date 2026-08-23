use super::support::{create, service, Worker};
use tempfile::TempDir;
use wt_control_protocol::{Operation, Response};

#[test]
fn reports_are_listed_counted_and_cleared_for_the_world_owner() {
    let temp = TempDir::new().unwrap();
    let Response::World { world } = service(&temp, Worker::default())
        .execute("tester", Operation::CreateWorld(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    wt_workload_registry::Registry::open(&temp.path().join("worlds.db"))
        .unwrap()
        .insert_agent_tool_report(
            world.world_id,
            wt_workload_registry::AgentToolReportKind::Bug,
            "job log was unavailable",
        )
        .unwrap();

    let Response::Worlds {
        agent_tool_report_counts,
        ..
    } = service(&temp, Worker::default())
        .execute("tester", Operation::ListWorlds)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(agent_tool_report_counts[&world.world_id], 1);

    let Response::AgentToolReports { reports } = service(&temp, Worker::default())
        .execute("tester", Operation::ListAgentToolReports)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].world_id, world.world_id);
    assert_eq!(reports[0].world_name.as_str(), "sample");
    assert_eq!(
        reports[0].kind,
        wt_control_protocol::AgentToolReportKind::Bug
    );
    assert_eq!(reports[0].description, "job log was unavailable");

    let Response::AgentToolReports { reports } = service(&temp, Worker::default())
        .execute("someone-else", Operation::ListAgentToolReports)
        .unwrap()
    else {
        panic!()
    };
    assert!(reports.is_empty());

    let Response::AgentToolReportsCleared { count } = service(&temp, Worker::default())
        .execute("tester", Operation::ClearAgentToolReports)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(count, 1);
    assert!(
        wt_workload_registry::Registry::open(&temp.path().join("worlds.db"))
            .unwrap()
            .list_agent_tool_reports("tester")
            .unwrap()
            .is_empty()
    );
}
