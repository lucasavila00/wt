// @generated automatically by Diesel CLI.

diesel::table! {
    codex_session_reports (world_id, session_id) {
        world_id -> Text,
        session_id -> Text,
        cwd -> Text,
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
    disks (id) {
        id -> Text,
    }
}

diesel::table! {
    guests (id) {
        id -> Text,
        kind -> Text,
        backend_id -> Text,
        disk_id -> Text,
        vcpus -> BigInt,
        memory_mib -> BigInt,
        disk_gib -> BigInt,
        compute_reserved -> Bool,
        disk_reserved_gib -> BigInt,
    }
}

diesel::table! {
    hosts (id) {
        id -> Text,
        gateway_grant_id -> Nullable<Text>,
    }
}

diesel::table! {
    runners (id) {
        id -> Text,
        name -> Text,
        status -> Text,
        github_runner_id -> Nullable<BigInt>,
        last_error -> Nullable<Text>,
    }
}

diesel::table! {
    worlds (id) {
        id -> Text,
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
    }
}

diesel::joinable!(guests -> disks (disk_id));
diesel::joinable!(agent_tool_reports -> worlds (world_id));
diesel::joinable!(codex_session_reports -> worlds (world_id));
diesel::joinable!(hosts -> worlds (id));
diesel::joinable!(runners -> guests (id));
diesel::joinable!(worlds -> guests (id));

diesel::allow_tables_to_appear_in_same_query!(
    agent_tool_reports,
    codex_session_reports,
    disks,
    guests,
    hosts,
    runners,
    worlds
);
