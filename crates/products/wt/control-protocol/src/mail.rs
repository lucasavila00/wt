use crate::WorldId;
use serde::{Deserialize, Serialize};

pub const MAX_WORLD_MAIL_PAGE_SIZE: u32 = 1000;
pub const MAX_MAIL_TEXT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailKind {
    Message,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldMail {
    pub id: u64,
    pub world_id: WorldId,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub pane_id: Option<String>,
    pub created_at_unix_ms: i64,
    pub kind: MailKind,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use crate::{ApiRequest, Operation, PROTOCOL_VERSION};

    #[test]
    fn list_request_has_a_stable_shape() {
        let world_id = "123e4567-e89b-12d3-a456-426614174000".parse().unwrap();
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::ListWorldMail {
                world_id,
                after_id: 42,
                limit: 100,
            }))
            .unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "list_world_mail",
                "world_id": "123e4567-e89b-12d3-a456-426614174000",
                "after_id": 42,
                "limit": 100
            })
        );
    }

    #[test]
    fn codex_requests_have_stable_shapes() {
        let world_id = "123e4567-e89b-12d3-a456-426614174000".parse().unwrap();
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::StartCodex {
                world_id,
                message: "review".into(),
            }))
            .unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "start_codex",
                "world_id": "123e4567-e89b-12d3-a456-426614174000",
                "message": "review"
            })
        );
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::InspectCodex {
                world_id,
                thread_id: "thread-123".into(),
            }))
            .unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "inspect_codex",
                "world_id": "123e4567-e89b-12d3-a456-426614174000",
                "thread_id": "thread-123"
            })
        );
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::SendCodexMessage {
                world_id,
                thread_id: "thread-123".into(),
                message: "continue".into(),
            }))
            .unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "send_codex_message",
                "world_id": "123e4567-e89b-12d3-a456-426614174000",
                "thread_id": "thread-123",
                "message": "continue"
            })
        );
    }
}
