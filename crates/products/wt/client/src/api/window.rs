use base64::Engine as _;
use serde::Serialize;
use wt_control_protocol::{Window, WindowOutputChannel, WindowState};

#[derive(Debug, Serialize)]
pub(super) struct ApiWindow {
    window_id: String,
    world_id: String,
    state: ApiWindowState,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_signal: Option<i32>,
    output: Vec<ApiWindowOutput>,
    next_after: u64,
    oldest_available: u64,
    output_gap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen: Option<ApiWindowScreen>,
}

#[derive(Debug, Serialize)]
struct ApiWindowOutput {
    record_id: u64,
    channel: ApiWindowOutputChannel,
    data_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiWindowOutputChannel {
    Stdout,
    Stderr,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiWindowState {
    Running,
    Exited,
    Stopped,
}

#[derive(Debug, Serialize)]
struct ApiWindowScreen {
    text: String,
    observed_at_unix_ms: i64,
}

impl From<Window> for ApiWindow {
    fn from(window: Window) -> Self {
        Self {
            window_id: window.window_id.to_string(),
            world_id: window.world_id.to_string(),
            state: match window.state {
                WindowState::Running => ApiWindowState::Running,
                WindowState::Exited => ApiWindowState::Exited,
                WindowState::Stopped => ApiWindowState::Stopped,
            },
            exit_code: window.exit_code,
            exit_signal: window.exit_signal,
            output: window
                .output
                .into_iter()
                .map(|record| ApiWindowOutput {
                    record_id: record.record_id,
                    channel: match record.channel {
                        WindowOutputChannel::Stdout => ApiWindowOutputChannel::Stdout,
                        WindowOutputChannel::Stderr => ApiWindowOutputChannel::Stderr,
                    },
                    data_base64: base64::engine::general_purpose::STANDARD.encode(record.data),
                })
                .collect(),
            next_after: window.next_after,
            oldest_available: window.oldest_available,
            output_gap: window.output_gap,
            screen: window.screen.map(|screen| ApiWindowScreen {
                text: screen.text,
                observed_at_unix_ms: screen.observed_at_unix_ms,
            }),
        }
    }
}
