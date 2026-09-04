use super::*;

#[test]
fn routing_fields_do_not_change_operation_identity() {
    let request = |context: &str, expected_server_id: Option<&str>| Request::DeleteWorld {
        api_version: API_VERSION,
        request_id: Uuid::new_v4().to_string(),
        expected_server_id: expected_server_id.map(str::to_owned),
        context: context.to_owned(),
        world_id: "00000000-0000-0000-0000-000000000001".to_owned(),
    };
    let (_, first) = request_to_operation(request("old-alias", None)).unwrap();
    let (_, second) = request_to_operation(request(
        "new-alias",
        Some("22222222-2222-4222-8222-222222222222"),
    ))
    .unwrap();

    assert_eq!(operation_hash(&first), operation_hash(&second));
}

#[test]
fn generated_request_types_enforce_wire_integer_widths_and_unknown_fields() {
    let request = serde_json::json!({
        "api_version": API_VERSION,
        "request_id": "11111111-1111-4111-8111-111111111111",
        "context": "ars",
        "operation": "create_world",
        "name": "agent-1",
        "vcpus": u32::MAX,
        "memory_mib": u64::MAX,
        "disk_gib": u64::MAX,
        "git_user_name": "Ada Lovelace",
        "git_user_email": "ada@example.com"
    });
    assert!(serde_json::from_value::<Request>(request.clone()).is_ok());

    let mut too_many_vcpus = request.clone();
    too_many_vcpus["vcpus"] = serde_json::json!(u64::from(u32::MAX) + 1);
    assert!(serde_json::from_value::<Request>(too_many_vcpus).is_err());

    let mut unknown = request;
    unknown["future_field"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Request>(unknown).is_err());
}
