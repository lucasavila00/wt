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
