use wt_control_protocol::{InstanceName, InstanceStatus};
use wt_end_to_end_tests::cmd;

#[path = "kvm/fixture.rs"]
mod fixture;
pub(crate) use fixture::*;
#[path = "kvm/binaries.rs"]
mod binaries;
#[path = "kvm/codex.rs"]
mod codex;
#[path = "kvm/gateway.rs"]
mod gateway;
#[path = "kvm/images.rs"]
mod images;
pub(crate) use codex::*;
#[path = "kvm/ssh.rs"]
mod ssh;
#[path = "kvm/support.rs"]
mod support;
pub(crate) use support::*;
#[path = "kvm/terminal.rs"]
mod terminal;
pub(crate) use terminal::*;
#[path = "../../../products/wt/client/tests/support/screen.rs"]
mod screen;
pub(crate) use screen::{Key, Screen};
#[path = "kvm/wt_shell.rs"]
mod wt_shell;
pub(crate) use wt_shell::{create_world_with_defaults, delete_world};

#[path = "kvm/git_tracking.rs"]
mod git_tracking;
#[path = "kvm/guest_lifecycle.rs"]
mod guest_lifecycle;
#[path = "kvm/shell.rs"]
mod shell;
