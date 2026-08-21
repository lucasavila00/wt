use std::io::Write;
use uuid::Uuid;
use wt_control_protocol::{AppSshAccess, WorldKind};
use wt_libvirt_kvm::WorkerError;

pub mod devcontainer;
pub mod host;
mod retained;

pub use retained::*;

pub enum ProvisionSpec<'a> {
    Devcontainer(devcontainer::ProvisionSpec<'a>),
    Host(host::ProvisionSpec<'a>),
}

#[derive(Clone, Debug)]
pub struct World {
    pub access: GuestAccess,
    pub application: WorldApplication,
}

#[derive(Clone, Debug)]
pub enum WorldApplication {
    Devcontainer { app_ssh: Option<AppSshAccess> },
    Host { setup_complete: bool },
}

impl WorldApplication {
    pub fn app_ssh(&self) -> Option<&AppSshAccess> {
        match self {
            Self::Devcontainer { app_ssh } => app_ssh.as_ref(),
            Self::Host { .. } => None,
        }
    }
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
pub struct Workers<D, H> {
    devcontainer: D,
    host: H,
}

impl<D, H> Workers<D, H> {
    pub fn new(devcontainer: D, host: H) -> Self {
        Self { devcontainer, host }
    }
}

impl<D, H> WorldWorker for Workers<D, H>
where
    D: devcontainer::WorldWorker + Clone + Send + Sync + 'static,
    H: host::WorldWorker + Clone + Send + Sync + 'static,
{
    fn provision(
        &self,
        spec: ProvisionSpec<'_>,
        log: &mut dyn Write,
    ) -> Result<World, WorkerError> {
        match spec {
            ProvisionSpec::Devcontainer(spec) => {
                self.devcontainer.provision(&spec, log).map(World::from)
            }
            ProvisionSpec::Host(spec) => self.host.provision(&spec, log).map(World::from),
        }
    }

    fn destroy(&self, kind: WorldKind, backend_id: &str, disk_id: Uuid) -> Result<(), WorkerError> {
        match kind {
            WorldKind::Devcontainer => self.devcontainer.destroy(backend_id, disk_id),
            WorldKind::Host => self.host.destroy(backend_id, disk_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn inspect(&self, kind: WorldKind, backend_id: &str) -> Result<WorldInspection, WorkerError> {
        match kind {
            WorldKind::Devcontainer => self
                .devcontainer
                .inspect(backend_id)
                .map(WorldInspection::from),
            WorldKind::Host => self.host.inspect(backend_id).map(WorldInspection::from),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn start(&self, kind: WorldKind, backend_id: &str) -> Result<World, WorkerError> {
        match kind {
            WorldKind::Devcontainer => self.devcontainer.start(backend_id).map(World::from),
            WorldKind::Host => self.host.start(backend_id).map(World::from),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn stop(&self, kind: WorldKind, backend_id: &str) -> Result<(), WorkerError> {
        match kind {
            WorldKind::Devcontainer => self.devcontainer.stop(backend_id),
            WorldKind::Host => self.host.stop(backend_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }

    fn disk_usage(&self, kind: WorldKind, disk_id: Uuid) -> Result<u64, WorkerError> {
        match kind {
            WorldKind::Devcontainer => self.devcontainer.disk_usage(disk_id),
            WorldKind::Host => self.host.disk_usage(disk_id),
            WorldKind::GithubCi => Err(WorkerError::new(
                "github-ci worlds are not owned by wt-server",
            )),
        }
    }
}

impl From<devcontainer::World> for World {
    fn from(world: devcontainer::World) -> Self {
        Self {
            access: world.access,
            application: WorldApplication::Devcontainer {
                app_ssh: world.app_ssh,
            },
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

impl From<devcontainer::WorldInspection> for WorldInspection {
    fn from(inspection: devcontainer::WorldInspection) -> Self {
        match inspection {
            devcontainer::WorldInspection::Missing => Self::Missing,
            devcontainer::WorldInspection::Running(world) => Self::Running(world.into()),
            devcontainer::WorldInspection::Stopped { reason } => Self::Stopped { reason },
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
