use std::fs;
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
#[path = "kvm/support.rs"]
mod support;
pub(crate) use support::*;
#[path = "kvm/host.rs"]
mod host;
pub(crate) use host::*;
#[path = "kvm/terminal.rs"]
mod terminal;
pub(crate) use terminal::*;

#[path = "kvm/guest_lifecycle.rs"]
mod guest_lifecycle;
