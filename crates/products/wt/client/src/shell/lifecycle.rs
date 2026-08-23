use super::control::ControlCommand;
use super::model::ShellModel;
use super::refresh::WorldRefresh;
use super::{delete, start_creation, ControlFlows};
use wt_client::config::ClientConfig;

pub(super) fn start_control_command(
    command: ControlCommand,
    config: &ClientConfig,
    refresh: &WorldRefresh,
    model: &ShellModel,
    flows: &mut ControlFlows,
) {
    if flows.creation.is_some() || flows.deletion.is_some() {
        return;
    }
    match command {
        ControlCommand::DeleteWorld => {
            flows.deletion = Some(delete::Flow::new(model.worlds().to_vec()));
        }
        ControlCommand::NewWorld => start_creation(
            command,
            config,
            refresh,
            model,
            &mut flows.creation,
            &mut flows.creation_error,
        ),
    }
}
