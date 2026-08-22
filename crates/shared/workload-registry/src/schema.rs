// @generated automatically by Diesel CLI.

diesel::table! {
    codex_session_catalog (session_id) {
        session_id -> Text,
        rollout_path -> Text,
        rollout_length -> BigInt,
        scan_offset -> BigInt,
        created_at_unix_ms -> Nullable<BigInt>,
        rollout_updated_at_unix_ms -> BigInt,
        title -> Nullable<Text>,
        title_from_user_message -> Bool,
        latest_user_message -> Nullable<Text>,
        latest_user_message_at_unix_ms -> Nullable<BigInt>,
        latest_agent_message -> Nullable<Text>,
        latest_agent_message_at_unix_ms -> Nullable<BigInt>,
        cwd -> Nullable<Text>,
        model -> Nullable<Text>,
        cli_version -> Nullable<Text>,
        turn_count -> BigInt,
        command_count -> BigInt,
        file_change_count -> BigInt,
        input_tokens -> BigInt,
        cached_input_tokens -> BigInt,
        output_tokens -> BigInt,
        reasoning_output_tokens -> BigInt,
    }
}

diesel::table! {
    codex_session_reports (world_id, session_id) {
        world_id -> Text,
        session_id -> Text,
        cwd -> Text,
        repository_root -> Nullable<Text>,
        repository_url -> Nullable<Text>,
        git_branch -> Nullable<Text>,
        tmux_session -> Text,
        pane_id -> Text,
        state -> Text,
        session_start_source -> Nullable<Text>,
        received_at_unix_ms -> BigInt,
    }
}

diesel::table! {
    agent_tool_reports (id) {
        id -> Integer,
        world_id -> Text,
        kind -> Text,
        description -> Text,
    }
}

diesel::table! {
    worlds (id) {
        id -> Text,
        backend_id -> Text,
        disk_id -> Text,
        vcpus -> BigInt,
        memory_mib -> BigInt,
        disk_gib -> BigInt,
        compute_reserved -> Bool,
        disk_reserved_gib -> BigInt,
        owner -> Text,
        name -> Text,
        status -> Text,
        guest_ip -> Nullable<Text>,
        last_error -> Nullable<Text>,
        setup_fingerprint -> Text,
        ssh_user -> Nullable<Text>,
        ssh_host -> Nullable<Text>,
        ssh_port -> Nullable<Integer>,
        ssh_host_keys -> Text,
        gateway_grant_id -> Nullable<Text>,
    }
}

diesel::joinable!(agent_tool_reports -> worlds (world_id));
diesel::joinable!(codex_session_reports -> worlds (world_id));

diesel::allow_tables_to_appear_in_same_query!(
    agent_tool_reports,
    codex_session_catalog,
    codex_session_reports,
    worlds
);
