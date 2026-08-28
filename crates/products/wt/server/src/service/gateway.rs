use wt_control_protocol::WorldId;

pub trait AgentToolGateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String>;
    fn revoke(&self, grant_id: &str) -> Result<(), String>;
    fn pane_observations(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String>;
    fn activate_pane_observations(&self, world_id: WorldId) -> Result<(), String>;
    fn deactivate_pane_observations(&self, world_id: WorldId) -> Result<(), String>;
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

    fn pane_observations(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String> {
        wt_agent_tool_gateway::Gateway::pane_observations(self, world_id)
            .map_err(|error| error.to_string())
    }

    fn activate_pane_observations(&self, world_id: WorldId) -> Result<(), String> {
        wt_agent_tool_gateway::Gateway::activate_pane_observations(self, world_id)
            .map_err(|error| error.to_string())
    }

    fn deactivate_pane_observations(&self, world_id: WorldId) -> Result<(), String> {
        wt_agent_tool_gateway::Gateway::deactivate_pane_observations(self, world_id)
            .map_err(|error| error.to_string())
    }
}
