use std::fs;
use wt_api::{InstanceName, InstanceStatus};
use wt_command::cmd;

#[path = "kvm/fixture.rs"]
mod fixture;
pub(crate) use fixture::*;
#[path = "kvm/binaries.rs"]
mod binaries;
#[path = "kvm/gateway.rs"]
mod gateway;
#[path = "kvm/images.rs"]
mod images;
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
#[path = "kvm/shared_folders.rs"]
mod shared_folders;
