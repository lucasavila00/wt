use std::io::Write;
use uuid::Uuid;
use wt_control_protocol::WorldKind;
use wt_libvirt_kvm::WorkerError;

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

pub mod host;
mod retained;

pub use retained::*;

pub enum ProvisionSpec<'a> {
    Host(host::ProvisionSpec<'a>),
}

#[derive(Clone, Debug)]
pub struct World {
    pub access: GuestAccess,
    pub application: WorldApplication,
}

#[derive(Clone, Debug)]
pub enum WorldApplication {
    Host { setup_complete: bool },
}

#[derive(Clone, Debug)]
pub enum WorldInspection {
    Missing,
    Running(World),
    Stopped { reason: Option<String> },
}

pub trait WorldWorker: Clone + Send + Sync + 'static {
    fn provision(&self, spec: ProvisionSpec<'_>, log: &mut dyn Write)
        -> Result<World, WorkerError>;
    fn destroy(&self, kind: WorldKind, backend_id: &str, disk_id: Uuid) -> Result<(), WorkerError>;
    fn inspect(&self, kind: WorldKind, backend_id: &str) -> Result<WorldInspection, WorkerError>;
    fn start(&self, kind: WorldKind, backend_id: &str) -> Result<World, WorkerError>;
    fn stop(&self, kind: WorldKind, backend_id: &str) -> Result<(), WorkerError>;
    fn disk_usage(&self, kind: WorldKind, disk_id: Uuid) -> Result<u64, WorkerError>;
}

#[derive(Clone)]
pub struct Workers<H> {
    host: H,
}

impl<H> Workers<H> {
    pub fn new(host: H) -> Self {
        Self { host }
    }
}

impl<H> WorldWorker for Workers<H>
where
    H: host::WorldWorker + Clone + Send + Sync + 'static,
{
    fn provision(
        &self,
        spec: ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        match spec {
            ProvisionSpec::Host(spec) => self.host.provision(&spec, log).map(World::from),
        }
    }

    fn destroy(&self, kind: WorldKind, backend_id: &str, disk_id: Uuid) -> Result<(), WorkerError> {
        match kind {
            WorldKind::Host => self.host.destroy(backend_id, disk_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn inspect(&self, kind: WorldKind, backend_id: &str) -> Result<WorldInspection, WorkerError> {
        match kind {
            WorldKind::Host => self.host.inspect(backend_id).map(WorldInspection::from),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn start(&self, kind: WorldKind, backend_id: &str) -> Result<World, WorkerError> {
        match kind {
            WorldKind::Host => self.host.start(backend_id).map(World::from),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn stop(&self, kind: WorldKind, backend_id: &str) -> Result<(), WorkerError> {
        match kind {
            WorldKind::Host => self.host.stop(backend_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn disk_usage(&self, kind: WorldKind, disk_id: Uuid) -> Result<u64, WorkerError> {
        match kind {
            WorldKind::Host => self.host.disk_usage(disk_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }
}

impl From<host::World> for World {
    fn from(world: host::World) -> Self {
        Self {
            access: world.access,
            application: WorldApplication::Host {
                setup_complete: world.setup_complete,
            },
        }
    }
}

impl From<host::WorldInspection> for WorldInspection {
    fn from(inspection: host::WorldInspection) -> Self {
        match inspection {
            host::WorldInspection::Missing => Self::Missing,
            host::WorldInspection::Running(world) => Self::Running(world.into()),
            host::WorldInspection::Stopped { reason } => Self::Stopped { reason },
        }
    }
}
