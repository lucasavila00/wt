use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{ApiError, PaneObservation, Response};

impl<W, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_pane_observations(&self, owner: &str) -> Result<Response, ApiError> {
        let worlds = self.store.list_owned(owner).map_err(map_store_error)?;
        let mut panes = Vec::new();
        for stored in worlds {
            let world = stored.world;
            let observations = self
                .gateway
                .pane_observations(world.world_id)
                .map_err(|error| {
                    ApiError::new(
                        wt_control_protocol::ErrorCode::Internal,
                        format!("read pane observations: {error}"),
                    )
                })?;
            panes.extend(observations.into_iter().map(|pane| PaneObservation {
                world_id: world.world_id,
                world_name: world.name.clone(),
                created_at_unix_ms: stored.created_at_unix_ms,
                tmux_session: pane.tmux_session,
                pane_id: pane.pane_id,
                cwd: pane.cwd,
                git_branch: pane.git_branch,
                changed_at_unix_ms: pane.changed_at_unix_ms,
                observed_at_unix_ms: pane.observed_at_unix_ms,
                render: pane.render,
            }));
        }
        Ok(Response::PaneObservations { panes })
    }
}
