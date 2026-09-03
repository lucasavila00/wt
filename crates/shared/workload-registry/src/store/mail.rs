use super::{map_registry_error, Store, StoreError};
use crate::{MailKind, WorldMail, WorldMailPage};
use uuid::Uuid;
use wt_world::WorldId;

impl Store {
    pub fn insert_world_mail(
        &self,
        world_id: WorldId,
        message: &str,
    ) -> Result<WorldMail, StoreError> {
        self.registry
            .insert_world_mail(world_id, message)
            .map_err(map_registry_error)
    }

    pub fn list_world_mail(
        &self,
        owner: &str,
        world_id: WorldId,
        after_id: u64,
        limit: u32,
    ) -> Result<WorldMailPage, StoreError> {
        self.registry
            .list_world_mail(owner, world_id, after_id, limit)
            .map_err(map_registry_error)
    }

    pub fn insert_codex_result(
        &self,
        world_id: WorldId,
        request_id: Uuid,
        session_id: Option<Uuid>,
        kind: MailKind,
        message: &str,
    ) -> Result<WorldMail, StoreError> {
        self.registry
            .insert_codex_result(world_id, request_id, session_id, kind, message)
            .map_err(map_registry_error)
    }
}
