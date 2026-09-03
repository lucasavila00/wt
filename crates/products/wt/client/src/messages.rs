use std::fmt::Write as _;
use std::io::Write as _;
use wt_client::config::{ClientConfig, Context};
use wt_client::transport::{self, ContextError};
use wt_control_protocol::{ApiRequest, Operation, Response, WorldMail, MAX_WORLD_MAIL_PAGE_SIZE};

pub fn show(config: &ClientConfig) -> anyhow::Result<()> {
    let result = list_all(config);
    if result.all_failed() {
        return Err(super::context_failures(
            "could not list world messages because every context failed",
            &result.failures,
            None,
        ));
    }
    print!("{}", format(&result.messages));
    std::io::stdout().flush()?;
    super::print_context_warnings(&result.failures);
    Ok(())
}

pub struct ContextWorldMail {
    context: String,
    mail: WorldMail,
}

pub struct ListResult {
    messages: Vec<ContextWorldMail>,
    failures: Vec<ContextError>,
    successful_reads: usize,
}

impl ListResult {
    fn all_failed(&self) -> bool {
        self.successful_reads == 0
    }
}

fn list_all(config: &ClientConfig) -> ListResult {
    let mut messages = Vec::new();
    let mut failures = Vec::new();
    let mut successful_reads = 0;
    for context in &config.contexts {
        let worlds = match transport::call(context, &ApiRequest::new(Operation::ListWorlds)) {
            Ok(Response::Worlds { worlds, .. }) => worlds,
            Ok(_) => {
                failures.push(transport::wrong_response(
                    context,
                    "list worlds for messages",
                ));
                continue;
            }
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        if worlds.is_empty() {
            successful_reads += 1;
        }
        for world in worlds {
            match list_world(context, world.world_id) {
                Ok(mail) => {
                    successful_reads += 1;
                    messages.extend(mail.into_iter().map(|mail| ContextWorldMail {
                        context: context.name.clone(),
                        mail,
                    }));
                }
                Err(error) => failures.push(error),
            }
        }
    }
    ListResult {
        messages,
        failures,
        successful_reads,
    }
}

fn list_world(
    context: &Context,
    world_id: wt_control_protocol::WorldId,
) -> Result<Vec<WorldMail>, ContextError> {
    let mut messages = Vec::new();
    let mut after_id = 0;
    let mut scan_high_water_id = None;
    loop {
        let response = transport::call(
            context,
            &ApiRequest::new(Operation::ListWorldMail {
                world_id,
                after_id,
                limit: MAX_WORLD_MAIL_PAGE_SIZE,
            }),
        )?;
        let Response::WorldMail {
            messages: mut page,
            high_water_id,
        } = response
        else {
            return Err(transport::wrong_response(context, "list world messages"));
        };
        let scan_high_water_id = *scan_high_water_id.get_or_insert(high_water_id);
        page.retain(|message| message.id <= scan_high_water_id);
        after_id = page.last().map_or(after_id, |message| message.id);
        let done = page.is_empty() || after_id >= scan_high_water_id;
        messages.extend(page);
        if done {
            return Ok(messages);
        }
    }
}

fn format(messages: &[ContextWorldMail]) -> String {
    if messages.is_empty() {
        return "No world messages.\n".to_owned();
    }
    let mut output = String::new();
    for message in messages {
        writeln!(
            output,
            "{}  {}  {}  {}",
            message.context,
            message.mail.world_id,
            message.mail.created_at_unix_ms,
            escape_message(&message.mail.message)
        )
        .expect("writing to a String cannot fail");
    }
    output
}

fn escape_message(message: &str) -> String {
    let mut escaped = String::new();
    for character in message.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\u{{{:x}}}", u32::from(character))
                    .expect("writing to a String cannot fail");
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_world_mail_as_stable_bounded_rows() {
        let messages = [ContextWorldMail {
            context: "local".into(),
            mail: WorldMail {
                id: 1,
                world_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
                request_id: None,
                session_id: None,
                created_at_unix_ms: 1_800_000_000_000,
                kind: wt_control_protocol::MailKind::Message,
                message: "done\nneeds\r\treview\\\0\u{1b} ✓".into(),
            },
        }];

        insta::assert_snapshot!(format(&messages), @"local  00000000-0000-0000-0000-000000000001  1800000000000  done\\nneeds\\r\\treview\\\\\\u{0}\\u{1b} ✓\n");
    }

    #[test]
    fn all_failed_depends_on_successful_reads_not_failure_count() {
        assert!(ListResult {
            messages: Vec::new(),
            failures: Vec::new(),
            successful_reads: 0,
        }
        .all_failed());
        assert!(!ListResult {
            messages: Vec::new(),
            failures: Vec::new(),
            successful_reads: 1,
        }
        .all_failed());
        assert!(!ListResult {
            messages: Vec::new(),
            failures: vec![transport::wrong_response(
                &Context {
                    name: "local".into(),
                    kind: wt_client::config::ContextKind::BareMetalLocal,
                },
                "list one world",
            )],
            successful_reads: 1,
        }
        .all_failed());
    }
}
