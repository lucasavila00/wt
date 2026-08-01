use std::fs;
use std::io::Write;
use std::process::Stdio;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use wt_api::{ForkInstance, InstanceName, InstanceStatus, Operation, Response};
use wt_command::cmd;

#[path = "kvm/fixture.rs"]
mod fixture;
pub(crate) use fixture::*;
#[path = "kvm/support.rs"]
mod support;
pub(crate) use support::*;

#[path = "kvm/fork_failures.rs"]
mod fork_failures;
#[path = "kvm/fork_graph.rs"]
mod fork_graph;
#[path = "kvm/fork_rejections.rs"]
mod fork_rejections;
#[path = "kvm/fork_runtime.rs"]
mod fork_runtime;
#[path = "kvm/guest_lifecycle.rs"]
mod guest_lifecycle;
