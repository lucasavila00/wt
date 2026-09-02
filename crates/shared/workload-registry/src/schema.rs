// @generated automatically by Diesel CLI.

diesel::table! {
    window_input (window_id, sequence_id) {
        window_id -> Text,
        sequence_id -> BigInt,
        data -> Binary,
    }
}

diesel::table! {
    window_output (window_id, record_id) {
        window_id -> Text,
        record_id -> BigInt,
        channel -> Text,
        data -> Binary,
    }
}

diesel::table! {
    windows (window_id) {
        window_id -> Text,
        world_id -> Text,
        owner -> Text,
        tmux_window_id -> Text,
        control_token_hash -> Text,
        argv_json -> Text,
        cwd -> Text,
        state -> Text,
        exit_code -> Nullable<Integer>,
        exit_signal -> Nullable<Integer>,
        next_output_record_id -> BigInt,
        oldest_available -> BigInt,
        retained_output_bytes -> BigInt,
        next_input_sequence_id -> BigInt,
        stdout_offset -> BigInt,
        stderr_offset -> BigInt,
        screen -> Nullable<Text>,
        screen_observed_at_unix_ms -> Nullable<BigInt>,
        created_at_unix_ms -> BigInt,
    }
}

diesel::table! {
    api_mutation_results (owner, request_id) {
        owner -> Text,
        request_id -> Text,
        request_hash -> Text,
        response_json -> Nullable<Text>,
        expires_at_unix_ms -> BigInt,
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
    server_metadata (singleton) {
        singleton -> Integer,
        server_id -> Text,
    }
}

diesel::table! {
    worlds (world_id) {
        world_id -> Text,
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
        created_at_unix_ms -> BigInt,
    }
}

diesel::table! {
    repositories (id) {
        id -> Integer,
        provider_host -> Text,
        repository -> Text,
    }
}

diesel::table! {
    world_git_activity (id) {
        id -> Integer,
        world_id -> Text,
        recorded_at_unix_ms -> BigInt,
        kind -> Text,
        repository_id -> Integer,
        git_service -> Nullable<Text>,
        branch -> Nullable<Text>,
        previous_oid -> Nullable<Text>,
        new_oid -> Nullable<Text>,
    }
}

diesel::table! {
    world_wt_tools_activity (id) {
        id -> Integer,
        world_id -> Text,
        recorded_at_unix_ms -> BigInt,
        repository_id -> Integer,
        action -> Text,
        branch -> Nullable<Text>,
        change_request -> Nullable<Text>,
        request_json -> Text,
        response_json -> Text,
    }
}

diesel::joinable!(agent_tool_reports -> worlds (world_id));
diesel::joinable!(world_git_activity -> repositories (repository_id));
diesel::joinable!(world_git_activity -> worlds (world_id));
diesel::joinable!(world_wt_tools_activity -> repositories (repository_id));
diesel::joinable!(world_wt_tools_activity -> worlds (world_id));
diesel::joinable!(window_input -> windows (window_id));
diesel::joinable!(window_output -> windows (window_id));
diesel::joinable!(windows -> worlds (world_id));

diesel::allow_tables_to_appear_in_same_query!(
    api_mutation_results,
    agent_tool_reports,
    repositories,
    server_metadata,
    world_git_activity,
    world_wt_tools_activity,
    worlds,
    window_input,
    window_output,
    windows
);
