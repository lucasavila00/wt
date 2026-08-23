use super::model::ShellModel;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;

pub(super) fn request_close(event: &Event, model: &mut ShellModel, area: Rect) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) || key.code != KeyCode::F(6)
    {
        return false;
    }
    let _ = model.handle_key(*key, area);
    true
}

pub(super) fn log_running_work(running: &[String]) {
    if let Some(message) = running_work_message(running) {
        eprintln!("{message}");
    }
}

fn running_work_message(running: &[String]) -> Option<String> {
    (!running.is_empty()).then(|| {
        format!(
            "wt shell closed with work still running: {}",
            running.join("; ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn f6_requests_close_before_blocking_flow_input() {
        let area = Rect::new(0, 0, 100, 30);
        let mut model = ShellModel::new(vec!["local.one".into()]);

        assert!(request_close(
            &Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            &mut model,
            area,
        ));
        assert!(model.should_quit());
    }

    #[test]
    fn running_work_message_lists_unfinished_actions() {
        insta::assert_snapshot!(
            running_work_message(&[
                "Create local.new-world (Provisioning guest)".into(),
                "Reconnect local.existing".into(),
            ])
            .unwrap(),
            @"wt shell closed with work still running: Create local.new-world (Provisioning guest); Reconnect local.existing"
        );
        assert_eq!(running_work_message(&[]), None);
    }
}
