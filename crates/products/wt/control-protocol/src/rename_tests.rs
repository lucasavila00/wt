use super::*;

#[test]
fn rename_request_carries_the_stable_world_id() {
    let request = ApiRequest::new(Operation::RenameWorld {
        world_id: WorldId::from(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()),
        new_name: WorldName::parse("new-name").unwrap(),
    });
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "protocol_version": 14,
            "operation": "rename_world",
            "world_id": "123e4567-e89b-12d3-a456-426614174000",
            "new_name": "new-name"
        })
    );
}
