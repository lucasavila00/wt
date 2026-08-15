use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use wt_api::{
    ApiError, ApiRequest, ApiResponse, Capacity, CapacityResource, ClientEffect,
    ClientEffectOutcome, ClientEffectOutput, ClientMessage, CreateApplication, CreateInstance,
    Instance, InstanceApplication, InstanceName, Operation, Outcome, OutputStream, Response,
    ServerMessage, WorldKind, CLIENT_SCHEMA_VERSION,
};

const DEFAULT_VCPUS: u32 = 2;
const DEFAULT_MEMORY_MIB: u64 = 4096;
const DEFAULT_DISK_GIB: u64 = 32;

#[derive(Debug, Parser)]
#[command(name = "wt")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a world.
    New {
        #[command(subcommand)]
        kind: Option<NewKind>,
    },
    /// List worlds on the selected server.
    Ls,
    /// Remove a world.
    Rm { name: InstanceName },
    /// Start a stopped world.
    Start { name: InstanceName },
    /// Open a world in VS Code Remote-SSH.
    Code { name: InstanceName },
    /// Update managed OpenSSH inventory.
    Sync,
}

#[derive(Debug, Subcommand)]
enum NewKind {
    /// Create a raw Ubuntu world from cloud-init user-data read on stdin.
    Host,
}

pub fn run(
    input: impl Read,
    output: impl Write,
    call: impl FnMut(ApiRequest) -> Result<ApiResponse>,
) -> Result<()> {
    let mut session = Session::new(input, output, call);
    let (schema, context, args) = match session.read_message().context("read start message")? {
        ClientMessage::Start {
            schema,
            context,
            args,
        } => (schema, context, args),
        _ => bail!("first client message is not start"),
    };
    if schema != CLIENT_SCHEMA_VERSION {
        session.send(&ServerMessage::SchemaMismatch {
            client_schema: schema,
            server_schema: CLIENT_SCHEMA_VERSION,
        })?;
        return Ok(());
    }
    session.context = context;
    session.send(&ServerMessage::Ready {
        schema: CLIENT_SCHEMA_VERSION,
    })?;

    let cli = match Cli::try_parse_from(std::iter::once("wt".to_owned()).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            let stream = if error.use_stderr() {
                OutputStream::Stderr
            } else {
                OutputStream::Stdout
            };
            session.output(stream, error.to_string())?;
            session.exit(if error.use_stderr() { 2 } else { 0 })?;
            return Ok(());
        }
    };

    match session.execute(cli.command) {
        Ok(()) => session.exit(0),
        Err(error) => {
            session.output(OutputStream::Stderr, format!("wt: {error:#}\n"))?;
            session.exit(1)
        }
    }
}

struct Session<R, W, F> {
    input: BufReader<R>,
    output: W,
    call: F,
    context: String,
    next_id: u64,
}

impl<R: Read, W: Write, F: FnMut(ApiRequest) -> Result<ApiResponse>> Session<R, W, F> {
    fn new(input: R, output: W, call: F) -> Self {
        Self {
            input: BufReader::new(input),
            output,
            call,
            context: String::new(),
            next_id: 1,
        }
    }

    fn read_message(&mut self) -> Result<ClientMessage> {
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            bail!("client closed the protocol stream")
        }
        serde_json::from_str(&line).context("decode client message")
    }

    fn send(&mut self, message: &ServerMessage) -> Result<()> {
        serde_json::to_writer(&mut self.output, message).context("encode server message")?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        Ok(())
    }

    fn output(&mut self, stream: OutputStream, text: impl Into<String>) -> Result<()> {
        self.send(&ServerMessage::Output {
            stream,
            text: text.into(),
        })
    }

    fn exit(&mut self, code: i32) -> Result<()> {
        self.send(&ServerMessage::Exit { code })
    }

    fn read_input(&mut self) -> Result<Option<String>> {
        let id = self.id();
        self.send(&ServerMessage::ReadInput { id })?;
        match self.read_message()? {
            ClientMessage::Input {
                id: response_id,
                text,
                eof,
            } if response_id == id => Ok((!eof).then_some(text)),
            ClientMessage::Input {
                id: response_id, ..
            } => {
                bail!("input result {response_id} does not match request {id}")
            }
            _ => bail!("client returned the wrong message for input request {id}"),
        }
    }

    fn prompt(&mut self, label: &str, default: Option<&str>) -> Result<String> {
        let suffix = default
            .map(|value| format!(" [{value}]: "))
            .unwrap_or_else(|| ": ".to_owned());
        loop {
            self.output(OutputStream::Stderr, format!("{label}{suffix}"))?;
            let value = self
                .read_input()?
                .context("standard input ended during interactive command")?;
            let value = value.trim_end_matches(['\r', '\n']);
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
            if let Some(default) = default {
                return Ok(default.to_owned());
            }
            self.output(OutputStream::Stderr, "a value is required\n")?;
        }
    }

    fn confirm(&mut self, label: &str, default: bool) -> Result<bool> {
        let hint = if default { "Y/n" } else { "y/N" };
        loop {
            let value = self.prompt(label, Some(hint))?;
            if value == hint {
                return Ok(default);
            }
            match value.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => self.output(OutputStream::Stderr, "enter yes or no\n")?,
            }
        }
    }

    fn effect(&mut self, effect: ClientEffect) -> Result<ClientEffectOutput> {
        let id = self.id();
        self.send(&ServerMessage::Effect { id, effect })?;
        match self.read_message()? {
            ClientMessage::EffectResult {
                id: response_id,
                outcome,
            } if response_id == id => match outcome {
                ClientEffectOutcome::Ok { output } => Ok(output),
                ClientEffectOutcome::Error { message } => bail!("{message}"),
            },
            ClientMessage::EffectResult {
                id: response_id, ..
            } => bail!("effect result {response_id} does not match request {id}"),
            _ => bail!("client returned the wrong message for effect {id}"),
        }
    }

    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn execute(&mut self, command: Command) -> Result<()> {
        match command {
            Command::New { kind } => self.create(kind),
            Command::Ls => {
                let instances = self.list()?;
                self.output(OutputStream::Stdout, format_instances(&instances))?;
                self.sync(instances)
            }
            Command::Rm { name } => {
                let response = self.response(Operation::Delete { name: name.clone() })?;
                if !matches!(response, Response::Deleted { .. }) {
                    bail!("daemon returned the wrong response to delete")
                }
                self.sync_current()?;
                self.output(OutputStream::Stdout, format!("removed {name}\n"))
            }
            Command::Start { name } => {
                let response = self.response(Operation::Start { name: name.clone() })?;
                let Response::Instance { instance } = response else {
                    bail!("daemon returned the wrong response to start")
                };
                self.sync_current()?;
                self.output(
                    OutputStream::Stdout,
                    format!("started {name} ({})\n", instance.status),
                )
            }
            Command::Code { name } => self.code(name),
            Command::Sync => {
                self.sync_current()?;
                self.output(OutputStream::Stdout, "updated managed SSH inventory\n")
            }
        }
    }

    fn create(&mut self, kind: Option<NewKind>) -> Result<()> {
        let name = loop {
            let value = self.prompt("World name", None)?;
            match InstanceName::parse(value) {
                Ok(name) => break name,
                Err(error) => self.output(OutputStream::Stderr, format!("{error}\n"))?,
            }
        };
        let application = match kind {
            None => {
                let (name, email) = match self.effect(ClientEffect::ReadGitIdentity)? {
                    ClientEffectOutput::GitIdentity { name, email } => (name, email),
                    _ => bail!("client returned the wrong Git identity effect result"),
                };
                let source = loop {
                    let value = self.prompt("Git repository", None)?;
                    match wt_api::validate_ssh_git_source(&value) {
                        Ok(()) => break value,
                        Err(error) => self.output(OutputStream::Stderr, format!("{error}\n"))?,
                    }
                };
                let git_base = loop {
                    let value = self.prompt("Base branch", Some("main"))?;
                    match wt_api::validate_git_branch(&value) {
                        Ok(()) => break value,
                        Err(error) => self.output(OutputStream::Stderr, format!("{error}\n"))?,
                    }
                };
                CreateApplication::Devcontainer {
                    source,
                    git_base,
                    git_user_name: name,
                    git_user_email: email,
                }
            }
            Some(NewKind::Host) => CreateApplication::Host {
                user_data: String::new(),
            },
        };
        let vcpus = self.prompt_number("Virtual CPUs", DEFAULT_VCPUS)?;
        let memory_mib = self.prompt_number("RAM (MiB)", DEFAULT_MEMORY_MIB)?;
        let disk_gib = self.prompt_number("Disk (GiB)", DEFAULT_DISK_GIB)?;
        let keys = match self.effect(ClientEffect::ReadSshPublicKeys)? {
            ClientEffectOutput::SshPublicKeys { keys } => keys,
            _ => bail!("client returned the wrong SSH public key effect result"),
        };
        if !self.confirm("Create this world?", true)? {
            bail!("creation cancelled")
        }
        let application = match application {
            CreateApplication::Host { .. } => {
                self.output(
                    OutputStream::Stderr,
                    "Cloud-init user-data; finish with end-of-file:\n",
                )?;
                let mut user_data = String::new();
                while let Some(input) = self.read_input()? {
                    user_data.push_str(&input);
                }
                if user_data.is_empty() {
                    bail!("cloud-init user-data is empty")
                }
                CreateApplication::Host { user_data }
            }
            application => application,
        };
        let request = CreateInstance {
            name: name.clone(),
            vcpus,
            memory_mib,
            disk_gib,
            ssh_authorized_keys: keys,
            application,
        };
        let instance = loop {
            match self.outcome(Operation::Create(request.clone()))? {
                Outcome::Ok { response } => {
                    let Response::Instance { instance } = *response else {
                        bail!("daemon returned the wrong response to create")
                    };
                    break *instance;
                }
                Outcome::Error { error }
                    if error.code == wt_api::ErrorCode::Capacity
                        && self.retry_capacity(&error)? => {}
                Outcome::Error { error } => return Err(api_error(&error)),
            }
        };
        self.sync_current().with_context(|| {
            format!(
                "world {}.{name} was created, but managed SSH inventory was not updated",
                self.context
            )
        })?;
        self.output(
            OutputStream::Stdout,
            format!(
                "{}.{}\t{}\t{}\n",
                self.context,
                instance.name,
                instance.status,
                instance.guest_ip.as_deref().unwrap_or("-")
            ),
        )?;
        self.effect(ClientEffect::ExecSsh {
            target: format!("{}.{}", self.context, instance.name),
        })?;
        Ok(())
    }

    fn prompt_number<T>(&mut self, label: &str, default: T) -> Result<T>
    where
        T: std::str::FromStr + std::fmt::Display + Copy + PartialEq + Default,
    {
        loop {
            let value = self.prompt(label, Some(&default.to_string()))?;
            match value.parse::<T>() {
                Ok(number) if number != T::default() => return Ok(number),
                _ => self.output(OutputStream::Stderr, "enter a number greater than zero\n")?,
            }
        }
    }

    fn retry_capacity(&mut self, error: &ApiError) -> Result<bool> {
        let capacity = error
            .capacity
            .as_ref()
            .context("server returned capacity error without details")?;
        self.output(OutputStream::Stderr, capacity_message(capacity))?;
        self.confirm("Retry after freeing capacity?", true)
    }

    fn code(&mut self, name: InstanceName) -> Result<()> {
        let response = self.response(Operation::Get { name: name.clone() })?;
        let Response::Instance { instance } = response else {
            bail!("daemon returned the wrong response to get")
        };
        if instance.status != wt_api::InstanceStatus::Running {
            bail!(
                "world {name} is {}; VS Code can only open a running world",
                instance.status
            )
        }
        if instance.kind() != WorldKind::Devcontainer {
            bail!(
                "world {name} is {}; VS Code only supports devcontainer worlds",
                instance.kind()
            )
        }
        if instance.ssh.is_none() || instance.application.app_ssh().is_none() {
            bail!("world {name} has incomplete SSH access information")
        }
        self.sync_current()?;
        self.effect(ClientEffect::LaunchCode {
            target: format!("{}.{}", self.context, name),
        })?;
        Ok(())
    }

    fn sync_current(&mut self) -> Result<()> {
        let instances = self.list()?;
        self.sync(instances)
    }

    fn sync(&mut self, instances: Vec<Instance>) -> Result<()> {
        match self.effect(ClientEffect::ReplaceSshInventory { instances })? {
            ClientEffectOutput::None => Ok(()),
            _ => bail!("client returned the wrong SSH inventory effect result"),
        }
    }

    fn list(&mut self) -> Result<Vec<Instance>> {
        let response = self.response(Operation::List)?;
        let Response::Instances { instances } = response else {
            bail!("daemon returned the wrong response to list")
        };
        Ok(instances)
    }

    fn response(&mut self, operation: Operation) -> Result<Response> {
        match self.outcome(operation)? {
            Outcome::Ok { response } => Ok(*response),
            Outcome::Error { error } => Err(api_error(&error)),
        }
    }

    fn outcome(&mut self, operation: Operation) -> Result<Outcome> {
        Ok((self.call)(ApiRequest::new(operation))?.outcome)
    }
}

fn api_error(error: &ApiError) -> anyhow::Error {
    anyhow::anyhow!("{}: {}", error_code(error.code), error.message)
}

fn error_code(code: wt_api::ErrorCode) -> &'static str {
    match code {
        wt_api::ErrorCode::InvalidRequest => "invalid request",
        wt_api::ErrorCode::UnsupportedProtocol => "unsupported protocol",
        wt_api::ErrorCode::Conflict => "conflict",
        wt_api::ErrorCode::NotFound => "not found",
        wt_api::ErrorCode::Capacity => "capacity unavailable",
        wt_api::ErrorCode::Backend => "backend error",
        wt_api::ErrorCode::Internal => "internal error",
    }
}

fn capacity_message(capacity: &Capacity) -> String {
    let unit = match capacity.resource {
        CapacityResource::Cpu => "CPU",
        CapacityResource::Memory => "MiB",
        CapacityResource::Disk => "GiB",
    };
    format!(
        "{} {unit} of {} {unit} reserved; request needs {} {unit}.\n",
        capacity.reserved, capacity.total, capacity.requested
    )
}

fn format_instances(instances: &[Instance]) -> String {
    let mut rows = vec![[
        "NAME".to_owned(),
        "KIND".to_owned(),
        "STATUS".to_owned(),
        "REPO".to_owned(),
        "RESOURCES".to_owned(),
        "DETAIL".to_owned(),
    ]];
    rows.extend(instances.iter().map(|instance| {
        [
            instance.name.to_string(),
            instance.kind().to_string(),
            instance.status.to_string(),
            match &instance.application {
                InstanceApplication::Devcontainer { source, .. } => {
                    repository_name(source).unwrap_or("-").to_owned()
                }
                InstanceApplication::Host => "-".to_owned(),
            },
            format_resources(instance.vcpus, instance.memory_mib, instance.disk_gib),
            instance.last_error.as_deref().unwrap_or("-").to_owned(),
        ]
    }));
    let mut widths = [0; 5];
    for row in &rows {
        for (width, value) in widths.iter_mut().zip(row) {
            *width = (*width).max(value.chars().count());
        }
    }
    let mut output = String::new();
    for row in rows {
        writeln!(
            output,
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            row[5],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
            w4 = widths[4],
        )
        .expect("writing to String cannot fail");
    }
    output
}

fn repository_name(source: &str) -> Option<&str> {
    let path = if let Some(rest) = source.strip_prefix("ssh://") {
        rest.split_once('/')?.1
    } else {
        source.split_once(':')?.1
    };
    let repository = path.trim_end_matches('/').rsplit('/').next()?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    (!repository.is_empty()).then_some(repository)
}

fn format_resources(vcpus: u32, memory_mib: u64, disk_gib: u64) -> String {
    let memory = if memory_mib.is_multiple_of(1024) {
        format!("{}G", memory_mib / 1024)
    } else {
        format!("{memory_mib}MiB")
    };
    format!("{vcpus} CPU · {memory} · {disk_gib}G")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_mismatch_stops_before_daemon_call() {
        let input = b"{\"message\":\"start\",\"schema\":9,\"context\":\"ars\",\"args\":[\"rm\",\"world\"]}\n";
        let mut output = Vec::new();
        run(input.as_slice(), &mut output, |_| {
            panic!("daemon was called")
        })
        .unwrap();
        insta::assert_snapshot!(String::from_utf8(output).unwrap(), @r#"{"message":"schema_mismatch","client_schema":9,"server_schema":1}"#);
    }

    #[test]
    fn list_renders_and_requests_inventory_effect() {
        let input = concat!(
            "{\"message\":\"start\",\"schema\":1,\"context\":\"ars\",\"args\":[\"ls\"]}\n",
            "{\"message\":\"effect_result\",\"id\":1,\"outcome\":\"ok\",\"output\":\"none\"}\n"
        );
        let mut output = Vec::new();
        run(input.as_bytes(), &mut output, |request| {
            assert!(matches!(request.operation, Operation::List));
            Ok(ApiResponse::ok(Response::Instances { instances: vec![] }))
        })
        .unwrap();
        insta::assert_snapshot!(String::from_utf8(output).unwrap(), @r###"
        {"message":"ready","schema":1}
        {"message":"output","stream":"stdout","text":"NAME  KIND  STATUS  REPO  RESOURCES  DETAIL\n"}
        {"message":"effect","id":1,"effect":"replace_ssh_inventory","instances":[]}
        {"message":"exit","code":0}
        "###);
    }
}
