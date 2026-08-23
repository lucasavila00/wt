use super::control::ControlCommand;
use super::model::ShellModel;
use super::{delete, start_creation, ControlFlows, ShellRuntime};

pub(super) fn start_control_command(
    command: ControlCommand,
    runtime: &ShellRuntime<'_>,
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
        ControlCommand::NewWorld => {
            start_creation(command, runtime.config, runtime.git_author, model, flows)
        }
    }
}
