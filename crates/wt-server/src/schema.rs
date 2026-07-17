// @generated automatically by Diesel CLI.

diesel::table! {
    instances (id) {
        id -> Text,
        owner -> Text,
        name -> Text,
        status -> Text,
        guest_ip -> Nullable<Text>,
        last_error -> Nullable<Text>,
        backend_id -> Text,
        source -> Text,
        vcpus -> BigInt,
        memory_mib -> BigInt,
        disk_gib -> BigInt,
        setup_fingerprint -> Text,
        ssh_user -> Nullable<Text>,
        ssh_host -> Nullable<Text>,
        ssh_port -> Nullable<Integer>,
        ssh_host_keys -> Text,
        app_ssh_user -> Nullable<Text>,
        app_ssh_port -> Nullable<Integer>,
        app_ssh_host_keys -> Text,
    }
}
