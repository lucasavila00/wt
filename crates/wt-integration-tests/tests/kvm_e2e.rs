use std::fs;
use wt_api::{InstanceName, InstanceStatus};
use wt_command::cmd;

#[path = "kvm/fixture.rs"]
mod fixture;
pub(crate) use fixture::*;
#[path = "kvm/support.rs"]
mod support;
pub(crate) use support::*;

#[path = "kvm/guest_lifecycle.rs"]
mod guest_lifecycle;
