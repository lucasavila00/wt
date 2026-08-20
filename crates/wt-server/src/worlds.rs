use std::io::Write;
use uuid::Uuid;
use wt_api::{AppSshAccess, WorldKind};
use wt_provider::WorkerError;
pub use wt_retained::GuestAccess;

pub enum ProvisionSpec<'a> {
    Devcontainer(wt_devcontainer::ProvisionSpec<'a>),
    Host(wt_host::ProvisionSpec<'a>),
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
    fn destroy(
        &self,
        kind: WorldKind,
        backend_id: &str,
        disk_ids: &[Uuid],
    ) -> Result<(), WorkerError>;
    fn inspect(&self, kind: WorldKind, backend_id: &str) -> Result<WorldInspection, WorkerError>;
    fn start(&self, kind: WorldKind, backend_id: &str) -> Result<World, WorkerError>;
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
    D: wt_devcontainer::WorldWorker + Clone + Send + Sync + 'static,
    H: wt_host::WorldWorker + Clone + Send + Sync + 'static,
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

    fn destroy(
        &self,
        kind: WorldKind,
        backend_id: &str,
        disk_ids: &[Uuid],
    ) -> Result<(), WorkerError> {
        match kind {
            WorldKind::Devcontainer => self.devcontainer.destroy(backend_id, disk_ids),
            WorldKind::Host => self.host.destroy(backend_id, disk_ids),
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
}

impl From<wt_devcontainer::World> for World {
    fn from(world: wt_devcontainer::World) -> Self {
        Self {
            access: world.access,
            application: WorldApplication::Devcontainer {
                app_ssh: world.app_ssh,
            },
        }
    }
}

impl From<wt_host::World> for World {
    fn from(world: wt_host::World) -> Self {
        Self {
            access: world.access,
            application: WorldApplication::Host {
                setup_complete: world.setup_complete,
            },
        }
    }
}

impl From<wt_devcontainer::WorldInspection> for WorldInspection {
    fn from(inspection: wt_devcontainer::WorldInspection) -> Self {
        match inspection {
            wt_devcontainer::WorldInspection::Missing => Self::Missing,
            wt_devcontainer::WorldInspection::Running(world) => Self::Running(world.into()),
            wt_devcontainer::WorldInspection::Stopped { reason } => Self::Stopped { reason },
        }
    }
}

impl From<wt_host::WorldInspection> for WorldInspection {
    fn from(inspection: wt_host::WorldInspection) -> Self {
        match inspection {
            wt_host::WorldInspection::Missing => Self::Missing,
            wt_host::WorldInspection::Running(world) => Self::Running(world.into()),
            wt_host::WorldInspection::Stopped { reason } => Self::Stopped { reason },
        }
    }
}
