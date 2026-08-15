use crate::Instance;
use serde::{Deserialize, Serialize};

pub const CLIENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "message", rename_all = "snake_case")]
pub enum ClientMessage {
    Start {
        schema: u32,
        context: String,
        args: Vec<String>,
    },
    Input {
        id: u64,
        text: String,
        eof: bool,
    },
    EffectResult {
        id: u64,
        #[serde(flatten)]
        outcome: ClientEffectOutcome,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum ClientEffect {
    ReadGitIdentity,
    ReadSshPublicKeys,
    ReplaceSshInventory { instances: Vec<Instance> },
    LaunchCode { target: String },
    ExecSsh { target: String },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ClientEffectOutcome {
    Ok {
        #[serde(flatten)]
        output: ClientEffectOutput,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "output", rename_all = "snake_case")]
pub enum ClientEffectOutput {
    None,
    GitIdentity { name: String, email: String },
    SshPublicKeys { keys: Vec<String> },
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "message", rename_all = "snake_case")]
pub enum ServerMessage {
    Ready {
        schema: u32,
    },
    SchemaMismatch {
        client_schema: u32,
        server_schema: u32,
    },
    Output {
        stream: OutputStream,
        text: String,
    },
    ReadInput {
        id: u64,
    },
    Effect {
        id: u64,
        #[serde(flatten)]
        effect: ClientEffect,
    },
    Exit {
        code: i32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_has_a_stable_shape() {
        let client = [
            ClientMessage::Start {
                schema: 1,
                context: "ars".into(),
                args: vec!["ls".into()],
            },
            ClientMessage::Input {
                id: 1,
                text: "answer\n".into(),
                eof: false,
            },
            ClientMessage::EffectResult {
                id: 2,
                outcome: ClientEffectOutcome::Ok {
                    output: ClientEffectOutput::GitIdentity {
                        name: "WT User".into(),
                        email: "wt@example.test".into(),
                    },
                },
            },
            ClientMessage::EffectResult {
                id: 3,
                outcome: ClientEffectOutcome::Ok {
                    output: ClientEffectOutput::SshPublicKeys {
                        keys: vec!["ssh-ed25519 AAAATEST".into()],
                    },
                },
            },
            ClientMessage::EffectResult {
                id: 4,
                outcome: ClientEffectOutcome::Error {
                    message: "failed".into(),
                },
            },
        ];
        let server = [
            ServerMessage::Ready { schema: 1 },
            ServerMessage::SchemaMismatch {
                client_schema: 1,
                server_schema: 2,
            },
            ServerMessage::Output {
                stream: OutputStream::Stdout,
                text: "hello\n".into(),
            },
            ServerMessage::ReadInput { id: 1 },
            ServerMessage::Effect {
                id: 2,
                effect: ClientEffect::ReadGitIdentity,
            },
            ServerMessage::Effect {
                id: 3,
                effect: ClientEffect::ReadSshPublicKeys,
            },
            ServerMessage::Effect {
                id: 4,
                effect: ClientEffect::ReplaceSshInventory { instances: vec![] },
            },
            ServerMessage::Effect {
                id: 5,
                effect: ClientEffect::LaunchCode {
                    target: "ars.world".into(),
                },
            },
            ServerMessage::Effect {
                id: 6,
                effect: ClientEffect::ExecSsh {
                    target: "ars.world".into(),
                },
            },
            ServerMessage::Exit { code: 0 },
        ];
        let mut encoded = String::new();
        for message in client {
            encoded.push_str(&serde_json::to_string(&message).unwrap());
            encoded.push('\n');
        }
        for message in server {
            encoded.push_str(&serde_json::to_string(&message).unwrap());
            encoded.push('\n');
        }
        insta::assert_snapshot!(encoded, @r###"
        {"message":"start","schema":1,"context":"ars","args":["ls"]}
        {"message":"input","id":1,"text":"answer\n","eof":false}
        {"message":"effect_result","id":2,"outcome":"ok","output":"git_identity","name":"WT User","email":"wt@example.test"}
        {"message":"effect_result","id":3,"outcome":"ok","output":"ssh_public_keys","keys":["ssh-ed25519 AAAATEST"]}
        {"message":"effect_result","id":4,"outcome":"error","message":"failed"}
        {"message":"ready","schema":1}
        {"message":"schema_mismatch","client_schema":1,"server_schema":2}
        {"message":"output","stream":"stdout","text":"hello\n"}
        {"message":"read_input","id":1}
        {"message":"effect","id":2,"effect":"read_git_identity"}
        {"message":"effect","id":3,"effect":"read_ssh_public_keys"}
        {"message":"effect","id":4,"effect":"replace_ssh_inventory","instances":[]}
        {"message":"effect","id":5,"effect":"launch_code","target":"ars.world"}
        {"message":"effect","id":6,"effect":"exec_ssh","target":"ars.world"}
        {"message":"exit","code":0}
        "###);
    }
}
