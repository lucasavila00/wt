use crate::{WorldId, WorldName};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wt_world::WindowId;

pub const MAX_WORLD_MAIL_PAGE_SIZE: u32 = 1000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldMail {
    pub id: u64,
    pub client_message_id: Uuid,
    pub world_id: WorldId,
    pub world_name: WorldName,
    pub window_id: WindowId,
    pub created_at_unix_ms: i64,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use crate::{ApiRequest, Operation, PROTOCOL_VERSION};
    use uuid::Uuid;

    #[test]
    fn request_has_a_stable_cursor_shape() {
        assert_eq!(
            serde_json::to_value(ApiRequest::new(Operation::ListWorldMail {
                world_id: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000")
                    .unwrap()
                    .into(),
                after_id: 41,
                limit: 100,
            }))
            .unwrap(),
            serde_json::json!({
                "protocol_version": PROTOCOL_VERSION,
                "operation": "list_world_mail",
                "world_id": "123e4567-e89b-12d3-a456-426614174000",
                "after_id": 41,
                "limit": 100,
            })
        );
    }
}
