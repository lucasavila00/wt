use crate::WorldId;
use serde::{Deserialize, Serialize};

pub const MAX_WORLD_MAIL_PAGE_SIZE: u32 = 1000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldMail {
    pub id: u64,
    pub world_id: WorldId,
    pub created_at_unix_ms: i64,
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
}
