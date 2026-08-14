// @generated automatically by Diesel CLI.

diesel::table! {
    devcontainers (id) {
        id -> Text,
        source -> Text,
        git_base -> Text,
        git_prefix -> Text,
        gateway_grant_id -> Text,
        app_ssh_user -> Nullable<Text>,
        app_ssh_port -> Nullable<Integer>,
        app_ssh_host_keys -> Text,
    }
}

diesel::table! {
    disk_nodes (id) {
        id -> Text,
        parent_id -> Nullable<Text>,
        immutable -> Bool,
    }
}

diesel::table! {
    guests (id) {
        id -> Text,
        kind -> Text,
        backend_id -> Text,
        head_disk_id -> Text,
        vcpus -> BigInt,
        memory_mib -> BigInt,
        disk_gib -> BigInt,
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

diesel::joinable!(guests -> disk_nodes (head_disk_id));
diesel::joinable!(devcontainers -> worlds (id));
diesel::joinable!(runners -> guests (id));
diesel::joinable!(worlds -> guests (id));

diesel::allow_tables_to_appear_in_same_query!(devcontainers, disk_nodes, guests, runners, worlds);
