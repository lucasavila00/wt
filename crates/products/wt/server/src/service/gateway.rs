use wt_control_protocol::WorldId;

pub trait AgentToolGrantAuthority {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String>;
    fn revoke(&self, world_id: WorldId) -> Result<(), String>;
}

pub trait LivePaneObservations {
    fn pane_observations(
        &self,
        world_id: WorldId,
    ) -> Result<Vec<wt_agent_tool_gateway::PaneObservationSnapshot>, String>;
    fn activate_pane_observations(&self, world_id: WorldId) -> Result<(), String>;
    fn deactivate_pane_observations(&self, world_id: WorldId) -> Result<(), String>;
}

impl AgentToolGrantAuthority for wt_agent_tool_gateway::Gateway {
    fn reserve(&self, world_id: WorldId) -> Result<wt_agent_tool_gateway::Grant, String> {
        self.reserve_grant(world_id.into())
            .map_err(|error| error.to_string())
    }

    fn revoke(&self, world_id: WorldId) -> Result<(), String> {
        self.revoke_grant(world_id.into())
            .map_err(|error| error.to_string())
    }
}

impl LivePaneObservations for wt_agent_tool_gateway::Gateway {
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
