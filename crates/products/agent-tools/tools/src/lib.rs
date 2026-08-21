mod api;

pub use api::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
}
