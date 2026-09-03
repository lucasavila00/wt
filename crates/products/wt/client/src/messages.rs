use std::fmt::Write as _;
use std::io::Write as _;
use wt_client::config::{ClientConfig, Context};
use wt_client::transport::{self, ContextError};
use wt_control_protocol::{
    ApiRequest, Operation, Response, WorldId, WorldMail, MAX_WORLD_MAIL_PAGE_SIZE,
};

pub fn show(config: &ClientConfig) -> anyhow::Result<()> {
    let result = list_all(config);
    if result.failures.len() == config.contexts.len() {
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

#[derive(Clone, Debug)]
pub struct ContextWorldMail {
    pub context: String,
    pub mail: WorldMail,
}

pub struct ListResult {
    pub messages: Vec<ContextWorldMail>,
    pub failures: Vec<ContextError>,
}

pub fn list_all(config: &ClientConfig) -> ListResult {
    let mut messages = Vec::new();
    let mut failures = Vec::new();
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
        let mut failed = false;
        for world in worlds {
            match list_world(context, world.world_id) {
                Ok(world_messages) => {
                    messages.extend(world_messages.into_iter().map(|mail| ContextWorldMail {
                        context: context.name.clone(),
                        mail,
                    }))
                }
                Err(error) => {
                    failures.push(error);
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            messages.retain(|message| message.context != context.name);
        }
    }
    ListResult { messages, failures }
}

pub fn list_world(context: &Context, world_id: WorldId) -> Result<Vec<WorldMail>, ContextError> {
    let mut messages = Vec::new();
    let mut after_id = 0;
    let mut high_water_id = None;
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
            messages: page,
            high_water_id: observed_high_water,
        } = response
        else {
            return Err(transport::wrong_response(context, "list world messages"));
        };
        let target = *high_water_id.get_or_insert(observed_high_water);
        let page_is_empty = page.is_empty();
        let page = page_through_high_water(page, target);
        after_id = page.last().map_or(after_id, |message| message.id);
        messages.extend(page);
        if after_id >= target || page_is_empty || observed_high_water == 0 {
            return Ok(messages);
        }
    }
}

fn page_through_high_water(page: Vec<WorldMail>, high_water_id: u64) -> Vec<WorldMail> {
    page.into_iter()
        .take_while(|message| message.id <= high_water_id)
        .collect()
}

pub fn format(messages: &[ContextWorldMail]) -> String {
    if messages.is_empty() {
        return "No world messages.\n".to_owned();
    }
    let mut rows = vec![[
        "CONTEXT".to_owned(),
        "WORLD".to_owned(),
        "WINDOW".to_owned(),
        "TIME (UNIX MS)".to_owned(),
        "MESSAGE".to_owned(),
    ]];
    rows.extend(messages.iter().map(|item| {
        let message = item
            .mail
            .message
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        [
            item.context.clone(),
            item.mail.world_name.to_string(),
            item.mail.window_id.to_string(),
            item.mail.created_at_unix_ms.to_string(),
            message,
        ]
    }));
    let mut widths = [0; 4];
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }
    let mut output = String::new();
    for row in rows {
        writeln!(
            output,
            "{:<context_width$}  {:<world_width$}  {:<window_width$}  {:<time_width$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            context_width = widths[0],
            world_width = widths[1],
            window_width = widths[2],
            time_width = widths[3],
        )
        .expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wt_control_protocol::{WindowId, WorldName};

    #[test]
    fn formats_messages_with_stable_identity_and_bounded_rows() {
        let messages = [ContextWorldMail {
            context: "local".into(),
            mail: WorldMail {
                id: 7,
                client_message_id: Uuid::nil(),
                world_id: Uuid::nil().into(),
                world_name: WorldName::parse("checkout").unwrap(),
                window_id: WindowId::from(
                    Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
                ),
                created_at_unix_ms: 1_788_374_400_000,
                message: "ready\nreview it".into(),
            },
        }];
        insta::assert_snapshot!(format(&messages), @r###"
        CONTEXT  WORLD     WINDOW                                TIME (UNIX MS)  MESSAGE
        local    checkout  11111111-1111-4111-8111-111111111111  1788374400000   ready\nreview it
        "###);
    }

    #[test]
    fn explains_an_empty_message_list() {
        insta::assert_snapshot!(format(&[]), @"No world messages.");
    }

    #[test]
    fn later_pages_stop_at_the_first_observed_high_water() {
        let first = message(7, "ready");
        let later = message(8, "arrived later");
        assert_eq!(
            page_through_high_water(vec![first.clone(), later], 7),
            vec![first]
        );
    }

    fn message(id: u64, message: &str) -> WorldMail {
        WorldMail {
            id,
            client_message_id: Uuid::nil(),
            world_id: Uuid::nil().into(),
            world_name: WorldName::parse("checkout").unwrap(),
            window_id: WindowId::from(
                Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            ),
            created_at_unix_ms: 1_788_374_400_000,
            message: message.into(),
        }
    }
}
