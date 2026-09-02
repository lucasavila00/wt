use serde::{Deserialize, Serialize};
use wt_world::{WindowId, WorldId};

pub const MAX_WINDOW_ARGV_ITEMS: usize = 256;
pub const MAX_WINDOW_ARG_BYTES: usize = 64 * 1024;
pub const MAX_WINDOW_CWD_BYTES: usize = 4096;
pub const MAX_WINDOW_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_WINDOW_OUTPUT_LIMIT: u32 = 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartWindow {
    pub world_id: WorldId,
    pub argv: Vec<String>,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<WindowId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_token: Option<String>,
}

impl StartWindow {
    pub fn validate(&self) -> Result<(), String> {
        if self.argv.is_empty() {
            return Err("window argv must not be empty".into());
        }
        if self.argv.len() > MAX_WINDOW_ARGV_ITEMS {
            return Err(format!("window argv exceeds {MAX_WINDOW_ARGV_ITEMS} items"));
        }
        let argv_bytes = self.argv.iter().try_fold(0usize, |total, argument| {
            if argument.as_bytes().contains(&0) {
                return Err("window argv must not contain NUL bytes".to_owned());
            }
            total
                .checked_add(argument.len())
                .ok_or_else(|| "window argv is too large".to_owned())
        })?;
        if argv_bytes > MAX_WINDOW_ARG_BYTES {
            return Err(format!("window argv exceeds {MAX_WINDOW_ARG_BYTES} bytes"));
        }
        if !self.cwd.starts_with('/') {
            return Err("window cwd must be an absolute path".into());
        }
        if self.cwd.len() > MAX_WINDOW_CWD_BYTES || self.cwd.as_bytes().contains(&0) {
            return Err("window cwd is invalid or too long".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    Running,
    Exited,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowOutputChannel {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowOutputRecord {
    pub record_id: u64,
    pub channel: WindowOutputChannel,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowScreen {
    pub text: String,
    pub observed_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub window_id: WindowId,
    pub world_id: WorldId,
    pub state: WindowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_signal: Option<i32>,
    pub output: Vec<WindowOutputRecord>,
    pub next_after: u64,
    pub oldest_available: u64,
    pub output_gap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<WindowScreen>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiRequest, Operation};
    use uuid::Uuid;

    #[test]
    fn managed_window_requests_have_stable_shapes() {
        let world_id =
            WorldId::from(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap());
        let window_id =
            WindowId::from(Uuid::parse_str("223e4567-e89b-12d3-a456-426614174000").unwrap());
        let requests = [
            ApiRequest::new(Operation::StartWindow(StartWindow {
                world_id,
                argv: vec!["cat".into()],
                cwd: "/home/wt".into(),
                window_id: None,
                control_token: None,
            })),
            ApiRequest::new(Operation::GetWindow {
                window_id,
                after: 4,
                limit: 20,
                include_screen: true,
            }),
            ApiRequest::new(Operation::SendWindowInput {
                window_id,
                control_token: "opaque".into(),
                data: vec![0, 255],
                api_request_id: None,
            }),
            ApiRequest::new(Operation::StopWindow {
                window_id,
                control_token: "opaque".into(),
            }),
            ApiRequest::new(Operation::DeleteWindow {
                window_id,
                control_token: "opaque".into(),
            }),
        ];
        insta::assert_snapshot!(serde_json::to_string_pretty(&requests).unwrap(), @r###"
        [
          {
            "protocol_version": 18,
            "operation": "start_window",
            "world_id": "123e4567-e89b-12d3-a456-426614174000",
            "argv": [
              "cat"
            ],
            "cwd": "/home/wt"
          },
          {
            "protocol_version": 18,
            "operation": "get_window",
            "window_id": "223e4567-e89b-12d3-a456-426614174000",
            "after": 4,
            "limit": 20,
            "include_screen": true
          },
          {
            "protocol_version": 18,
            "operation": "send_window_input",
            "window_id": "223e4567-e89b-12d3-a456-426614174000",
            "control_token": "opaque",
            "data": [
              0,
              255
            ]
          },
          {
            "protocol_version": 18,
            "operation": "stop_window",
            "window_id": "223e4567-e89b-12d3-a456-426614174000",
            "control_token": "opaque"
          },
          {
            "protocol_version": 18,
            "operation": "delete_window",
            "window_id": "223e4567-e89b-12d3-a456-426614174000",
            "control_token": "opaque"
          }
        ]
        "###);
    }
}
