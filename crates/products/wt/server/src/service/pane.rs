use super::{map_store_error, AgentToolGateway, Service};
use wt_control_protocol::{ApiError, PaneObservation, Response, WorldName};
use wt_guest::WorldWorker;

impl<W: WorldWorker, G: AgentToolGateway> Service<W, G> {
    pub(super) fn list_pane_observations(&self, owner: &str) -> Result<Response, ApiError> {
        let panes = self
            .store
            .list_pane_observations(owner)
            .map_err(map_store_error)?
            .into_iter()
            .map(|pane| {
                let frame = self
                    .gateway
                    .pane_frames(pane.world_id)
                    .map_err(|error| {
                        ApiError::new(
                            wt_control_protocol::ErrorCode::Internal,
                            format!("read pane frame: {error}"),
                        )
                    })?
                    .into_iter()
                    .find(|frame| {
                        frame.tmux_session == pane.tmux_session
                            && frame.pane_id == pane.pane_id
                            && frame.observed_at_unix_ms == pane.observed_at_unix_ms
                    })
                    .map(|frame| frame.frame);
                Ok(PaneObservation {
                    world_id: pane.world_id,
                    world_name: WorldName::parse(pane.world_name).map_err(|error| {
                        ApiError::new(
                            wt_control_protocol::ErrorCode::Internal,
                            format!("invalid pane world: {error}"),
                        )
                    })?,
                    created_at_unix_ms: pane.created_at_unix_ms,
                    tmux_session: pane.tmux_session,
                    pane_id: pane.pane_id,
                    window_index: pane.window_index,
                    window_name: pane.window_name,
                    cwd: pane.cwd,
                    git_branch: pane.git_branch,
                    changed_at_unix_ms: pane.changed_at_unix_ms,
                    observed_at_unix_ms: pane.observed_at_unix_ms,
                    frame,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok(Response::PaneObservations { panes })
    }
}
