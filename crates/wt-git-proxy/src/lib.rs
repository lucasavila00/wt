//! Standalone OpenSSH frontend and operator tooling for the shared Git proxy.

mod admin;
mod cli;
mod config;
mod service;
mod tui;

pub use admin::{
    add_generated_client, add_public_key, list_keys, remove_key, AuthorizedKey, GeneratedClient,
};
pub use cli::run;
pub use config::{ProviderConfig, ProxyConfig};
pub use service::serve;
pub use tui::run_tui;
