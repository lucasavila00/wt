use super::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn snapshots_the_world_creation_review_screen() {
    let backend = TestBackend::new(84, 22);
    let mut terminal = Terminal::new(backend).unwrap();
    let form = Form {
        contexts: vec!["local".into(), "lab".into()],
        capacities: std::collections::BTreeMap::new(),
        context: 1,
        name: "repo-feature".into(),
        name_is_suggestion: false,
        vcpus: "4".into(),
        memory: "8192".into(),
        disk: "64".into(),
        author: GitAuthor {
            name: "Test User".into(),
            email: "test@example.com".into(),
        },
        focus: OK_FOCUS,
        stage: Stage::Review,
        error: None,
    };
    terminal
        .draw(|frame| form.render(frame, frame.area()))
        .unwrap();
    let contents = terminal
        .backend()
        .buffer()
        .content()
        .chunks(84)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(contents);
}
