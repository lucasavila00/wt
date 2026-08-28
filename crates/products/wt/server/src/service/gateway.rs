use wt_control_protocol::WorldId;

pub trait AgentToolGateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String>;
    fn revoke(&self, grant_id: &str) -> Result<(), String>;
    fn pane_frames(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneFrameSnapshot>, String>;
    fn clear_pane_frames(&self, world_id: WorldId) -> Result<(), String>;
}

impl AgentToolGateway for wt_agent_tool_gateway::ControlClient {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String> {
        let response = self
            .request(&wt_agent_tool_gateway::ControlRequest::Reserve {
                world_id: world_id.to_string(),
            })
            .map_err(|error| error.to_string())?;
        if response.ok {
            response
                .grant
                .ok_or_else(|| "gateway reserve response has no grant".to_owned())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "gateway rejected grant".to_owned()))
        }
    }

    fn revoke(&self, grant_id: &str) -> Result<(), String> {
        let response = self
            .request(&wt_agent_tool_gateway::ControlRequest::Revoke {
                grant_id: grant_id.to_owned(),
            })
            .map_err(|error| error.to_string())?;
        if response.ok {
            Ok(())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "gateway rejected revocation".to_owned()))
        }
    }

    fn pane_frames(
        &self,
        _world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneFrameSnapshot>, String> {
        Ok(Vec::new())
    }

    fn clear_pane_frames(&self, _world_id: WorldId) -> Result<(), String> {
        Ok(())
    }
}

impl AgentToolGateway for wt_agent_tool_gateway::Gateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String> {
        self.reserve_grant(world_id.into())
            .map_err(|error| error.to_string())
    }

    fn revoke(&self, grant_id: &str) -> Result<(), String> {
        self.revoke_grant(grant_id)
            .map_err(|error| error.to_string())
    }

    fn pane_frames(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneFrameSnapshot>, String> {
        wt_agent_tool_gateway::Gateway::pane_frames(self, world_id)
            .map_err(|error| error.to_string())
    }

    fn clear_pane_frames(&self, world_id: WorldId) -> Result<(), String> {
        wt_agent_tool_gateway::Gateway::clear_pane_frames(self, world_id)
            .map_err(|error| error.to_string())
    }
}
