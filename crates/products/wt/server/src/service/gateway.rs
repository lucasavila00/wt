use uuid::Uuid;

pub trait AgentToolGateway {
    fn reserve(&self, world_id: Uuid) -> Result<wt_agent_tool_gateway::Grant, String>;
    fn revoke(&self, grant_id: &str) -> Result<(), String>;
}

impl AgentToolGateway for wt_agent_tool_gateway::ControlClient {
    fn reserve(&self, world_id: Uuid) -> Result<wt_agent_tool_gateway::Grant, String> {
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
}
