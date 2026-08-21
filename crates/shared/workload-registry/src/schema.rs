// @generated automatically by Diesel CLI.

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

diesel::allow_tables_to_appear_in_same_query!(agent_tool_reports, codex_session_reports, worlds);
