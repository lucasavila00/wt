use super::support::{create, service, Worker};
use tempfile::TempDir;
use wt_control_protocol::{Operation, Response};

#[test]
fn reports_are_listed_counted_and_cleared_for_the_world_owner() {
    let temp = TempDir::new().unwrap();
    let Response::Instance { instance } = service(&temp, Worker::default())
        .execute("tester", Operation::Create(create("sample")))
        .unwrap()
    else {
        panic!()
    };
    wt_workload_registry::Registry::open(&temp.path().join("instances.db"))
        .unwrap()
        .insert_agent_tool_report(
            instance.id,
            wt_workload_registry::AgentToolReportKind::Bug,
            "job log was unavailable",
        )
        .unwrap();

    let Response::Instances {
        agent_tool_report_counts,
        ..
    } = service(&temp, Worker::default())
        .execute("tester", Operation::List)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(agent_tool_report_counts[&instance.id], 1);

    let Response::AgentToolReports { reports } = service(&temp, Worker::default())
        .execute("tester", Operation::ListAgentToolReports)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].world_id, instance.id);
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
        wt_workload_registry::Registry::open(&temp.path().join("instances.db"))
            .unwrap()
            .list_agent_tool_reports("tester")
            .unwrap()
            .is_empty()
    );
}
