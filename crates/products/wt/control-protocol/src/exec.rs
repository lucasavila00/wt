use serde::{Deserialize, Serialize};

/// One bounded UTF-8 command exchange. No shell expansion and no replay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecCommand {
    pub executable: String,
    pub args: Vec<String>,
    pub stdin: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: i64,
}
