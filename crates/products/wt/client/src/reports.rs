use std::fmt::Write as _;
use std::io::Write as _;
use wt_control_protocol::{AgentGitReport, ApiRequest, Operation, Response};
use wt_client::config::ClientConfig;
use wt_client::transport::{self, ContextError};

pub fn show(config: &ClientConfig) -> anyhow::Result<()> {
    let result = list_all(config);
    if result.failures.len() == config.contexts.len() {
        return Err(super::context_failures(
            "could not list ag-git reports because every context failed",
            &result.failures,
            None,
        ));
    }
    print!("{}", format(&result.reports));
    std::io::stdout().flush()?;
    super::print_context_warnings(&result.failures);
    Ok(())
}

pub fn clear(config: &ClientConfig) -> anyhow::Result<()> {
    let result = clear_all(config);
    if result.failures.len() == config.contexts.len() {
        return Err(super::context_failures(
            "could not clear ag-git reports because every context failed",
            &result.failures,
            None,
        ));
    }
    println!(
        "cleared {} ag-git report{}",
        result.count,
        if result.count == 1 { "" } else { "s" }
    );
    super::print_context_warnings(&result.failures);
    Ok(())
}

#[derive(Debug)]
pub struct ContextAgentGitReport {
    context: String,
    report: AgentGitReport,
}

pub struct ListResult {
    pub reports: Vec<ContextAgentGitReport>,
    pub failures: Vec<ContextError>,
}

pub struct ClearResult {
    pub count: u64,
    pub failures: Vec<ContextError>,
}

pub fn list_all(config: &ClientConfig) -> ListResult {
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for context in &config.contexts {
        match transport::call(context, &ApiRequest::new(Operation::ListAgentGitReports)) {
            Ok(Response::AgentGitReports {
                reports: context_reports,
            }) => reports.extend(
                context_reports
                    .into_iter()
                    .map(|report| ContextAgentGitReport {
                        context: context.name.clone(),
                        report,
                    }),
            ),
            Ok(_) => failures.push(transport::wrong_response(context, "list ag-git reports")),
            Err(error) => failures.push(error),
        }
    }
    ListResult { reports, failures }
}

pub fn clear_all(config: &ClientConfig) -> ClearResult {
    let mut count = 0;
    let mut failures = Vec::new();
    for context in &config.contexts {
        match transport::call(context, &ApiRequest::new(Operation::ClearAgentGitReports)) {
            Ok(Response::AgentGitReportsCleared {
                count: context_count,
            }) => count += context_count,
            Ok(_) => failures.push(transport::wrong_response(context, "clear ag-git reports")),
            Err(error) => failures.push(error),
        }
    }
    ClearResult { count, failures }
}

pub fn format(reports: &[ContextAgentGitReport]) -> String {
    if reports.is_empty() {
        return "No ag-git reports.\n".to_owned();
    }
    let mut rows = vec![[
        "CONTEXT".to_owned(),
        "WORLD".to_owned(),
        "KIND".to_owned(),
        "DESCRIPTION".to_owned(),
    ]];
    rows.extend(reports.iter().map(|item| {
        let description = item
            .report
            .description
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        [
            item.context.clone(),
            item.report.world_name.to_string(),
            item.report.kind.to_string(),
            description,
        ]
    }));
    let mut widths = [0; 3];
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }
    let mut output = String::new();
    for row in rows {
        writeln!(
            output,
            "{:<context_width$}  {:<world_width$}  {:<kind_width$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            context_width = widths[0],
            world_width = widths[1],
            kind_width = widths[2],
        )
        .expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_control_protocol::{AgentGitReport, AgentGitReportKind, InstanceName};

    #[test]
    fn formats_reports_as_bounded_rows() {
        let reports = vec![
            ContextAgentGitReport {
                context: "local".into(),
                report: AgentGitReport {
                    world_id: Uuid::nil(),
                    world_name: InstanceName::parse("checkout").unwrap(),
                    kind: AgentGitReportKind::Bug,
                    description: "log output is missing\nfor failed jobs".into(),
                },
            },
            ContextAgentGitReport {
                context: "lab".into(),
                report: AgentGitReport {
                    world_id: Uuid::nil(),
                    world_name: InstanceName::parse("review").unwrap(),
                    kind: AgentGitReportKind::FeatureRequest,
                    description: "support commit search".into(),
                },
            },
        ];

        insta::assert_snapshot!(format(&reports), @r###"
        CONTEXT  WORLD     KIND             DESCRIPTION
        local    checkout  bug              log output is missing\nfor failed jobs
        lab      review    feature request  support commit search
        "###);
    }

    #[test]
    fn explains_an_empty_report_list() {
        insta::assert_snapshot!(format(&[]), @"No ag-git reports.");
    }
}
