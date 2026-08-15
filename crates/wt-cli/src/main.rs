use anyhow::{bail, Context as _, Result};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use wt_api::{
    ClientEffectOutcome, ClientMessage, OutputStream, ServerMessage, CLIENT_SCHEMA_VERSION,
};
use wt_cli::config::{ClientConfig, Context, ContextKind};

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("wt: {error:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    let config = ClientConfig::load()?;
    let args = std::env::args_os()
        .skip(1)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("command arguments must be UTF-8"))
        })
        .collect::<Result<Vec<_>>>()?;
    let (context, args) = select_context(&config, args)?;
    converse(&config, context, args)
}

fn select_context(config: &ClientConfig, args: Vec<String>) -> Result<(&Context, Vec<String>)> {
    let mut selected = None;
    let mut forwarded = Vec::new();
    let mut arguments = args.into_iter();
    while let Some(argument) = arguments.next() {
        let context = if argument == "--ctx" {
            Some(arguments.next().context("--ctx requires a context name")?)
        } else {
            argument.strip_prefix("--ctx=").map(str::to_owned)
        };
        if let Some(context) = context {
            if selected.replace(context).is_some() {
                bail!("--ctx may be specified only once")
            }
        } else {
            forwarded.push(argument);
        }
    }
    let name = match selected {
        Some(name) => name,
        None if config.contexts.len() == 1 => config.contexts[0].name.clone(),
        None => bail!("multiple contexts are configured; use `wt --ctx NAME COMMAND`"),
    };
    let context = config
        .context(&name)
        .ok_or_else(|| anyhow::anyhow!("unknown context: {name}"))?;
    Ok((context, forwarded))
}

fn converse(config: &ClientConfig, context: &Context, args: Vec<String>) -> Result<i32> {
    let mut child = helper_command(context)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start context helper")?;
    let mut helper_input = child.stdin.take().context("context helper has no stdin")?;
    let helper_output = child
        .stdout
        .take()
        .context("context helper has no stdout")?;
    let mut helper_output = BufReader::new(helper_output);
    send(
        &mut helper_input,
        &ClientMessage::Start {
            schema: CLIENT_SCHEMA_VERSION,
            context: context.name.clone(),
            args,
        },
    )?;

    let mut ready = false;
    loop {
        let message: ServerMessage = read(&mut helper_output)?;
        match message {
            ServerMessage::Ready { schema } => {
                if ready {
                    bail!("context helper sent ready more than once")
                }
                if schema != CLIENT_SCHEMA_VERSION {
                    bail!(
                        "context helper uses client schema {schema}; expected {CLIENT_SCHEMA_VERSION}"
                    )
                }
                ready = true;
            }
            ServerMessage::SchemaMismatch {
                client_schema,
                server_schema,
            } => bail!(
                "client schema {client_schema} does not match server schema {server_schema}; upgrade the wt client"
            ),
            ServerMessage::Output { stream, text } => {
                require_ready(ready)?;
                match stream {
                    OutputStream::Stdout => {
                        std::io::stdout().write_all(text.as_bytes())?;
                        std::io::stdout().flush()?;
                    }
                    OutputStream::Stderr => {
                        std::io::stderr().write_all(text.as_bytes())?;
                        std::io::stderr().flush()?;
                    }
                }
            }
            ServerMessage::ReadInput { id } => {
                require_ready(ready)?;
                let mut text = String::new();
                let eof = std::io::stdin().read_line(&mut text)? == 0;
                send(&mut helper_input, &ClientMessage::Input { id, text, eof })?;
            }
            ServerMessage::Effect { id, effect } => {
                require_ready(ready)?;
                let outcome = match wt_cli::effects::execute(config, context, effect) {
                    Ok(output) => ClientEffectOutcome::Ok { output },
                    Err(error) => ClientEffectOutcome::Error {
                        message: format!("{error:#}"),
                    },
                };
                send(
                    &mut helper_input,
                    &ClientMessage::EffectResult { id, outcome },
                )?;
            }
            ServerMessage::Exit { code } => {
                require_ready(ready)?;
                if !(0..=255).contains(&code) {
                    bail!("context helper returned invalid exit code {code}")
                }
                drop(helper_input);
                let status = child.wait().context("wait for context helper")?;
                if !status.success() {
                    bail!("context helper exited with {status}")
                }
                return Ok(code);
            }
        }
    }
}

fn require_ready(ready: bool) -> Result<()> {
    if !ready {
        bail!("context helper sent a command message before schema negotiation")
    }
    Ok(())
}

fn send(output: &mut impl Write, message: &ClientMessage) -> Result<()> {
    serde_json::to_writer(&mut *output, message).context("encode client message")?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

fn read(input: &mut impl BufRead) -> Result<ServerMessage> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        bail!("context helper closed the protocol stream without an exit message")
    }
    serde_json::from_str(&line).with_context(|| format!("decode context helper message: {line}"))
}

fn helper_command(context: &Context) -> Command {
    match &context.kind {
        ContextKind::BareMetalLocal => {
            let mut command = Command::new("wt-server");
            command.arg("api");
            command
        }
        ContextKind::BareMetalSsh { host } => {
            let mut command = Command::new("ssh");
            command.args(["--", host, "wt-server", "api"]);
            command
        }
    }
}
