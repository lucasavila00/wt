use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use wt_control_protocol::{WindowId, WindowOutputChannel, WindowState};

const STATE_ROOT: &str = "/home/wt/.local/state/wt/windows";
const MAX_OBSERVATION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    window_id: WindowId,
    argv: Vec<String>,
    cwd: String,
}

#[derive(Serialize)]
struct Started {
    tmux_window_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    window_id: WindowId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObserveRequest {
    window_id: WindowId,
    output_offset: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputRequest {
    window_id: WindowId,
    sequence_id: u64,
    data: Vec<u8>,
}

#[derive(Clone, Serialize)]
struct OutputChunk {
    channel: WindowOutputChannel,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct Observation {
    tmux_window_id: String,
    state: WindowState,
    exit_code: Option<i32>,
    exit_signal: Option<i32>,
    output_offset: u64,
    output: Vec<OutputChunk>,
    screen: String,
    screen_observed_at_unix_ms: i64,
}

#[derive(Clone, Deserialize, Serialize)]
struct Status {
    state: WindowState,
    exit_code: Option<i32>,
    exit_signal: Option<i32>,
}

pub fn run(action: &str) -> Result<()> {
    match action {
        "window-start" => start(read_json()?),
        "window-observe" => observe(read_json()?),
        "window-input" => input(read_json()?),
        "window-stop" => stop(read_json()?),
        "window-delete" => delete(read_json()?),
        "window-stop-all" => stop_all(),
        _ => bail!("unknown managed-window action {action:?}"),
    }
}

pub fn run_process(id: &str) -> Result<()> {
    let window_id: WindowId = id.parse().context("parse window ID")?;
    let directory = window_dir(window_id);
    let launch: Launch = serde_json::from_slice(
        &fs::read(directory.join("launch.json")).context("read window launch")?,
    )
    .context("parse window launch")?;
    if let Ok(pane_id) = std::env::var("TMUX_PANE") {
        let output = Command::new("/usr/bin/tmux")
            .args(["set-option", "-p", "-t", &pane_id, "remain-on-exit", "on"])
            .output()?;
        command_success("retain completed managed pane", &output)?;
    }
    let mut command = Command::new(&launch.argv[0]);
    command
        .args(&launch.argv[1..])
        .current_dir(&launch.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().context("start managed executable")?;
    let pid = child.id();
    let identity = process_identity(pid)?.context("managed executable exited during startup")?;
    if identity.process_group != pid {
        let _ = child.kill();
        bail!("managed executable did not create its own process group");
    }
    fs::write(
        directory.join("pid"),
        format!("{pid} {}\n", identity.start_time),
    )
    .context("write window process identity")?;
    write_status(
        &directory,
        &Status {
            state: WindowState::Running,
            exit_code: None,
            exit_signal: None,
        },
    )?;
    let event_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("output.bin"))
            .context("open window output")?,
    ));
    let stdout = child.stdout.take().context("take executable stdout")?;
    let stderr = child.stderr.take().context("take executable stderr")?;
    let input = child.stdin.take().context("take executable stdin")?;
    let input_socket = directory.join("input.sock");
    let _ = fs::remove_file(&input_socket);
    let listener = UnixListener::bind(&input_socket).context("bind window input socket")?;
    let input_state = directory.join("last-input-sequence");
    std::thread::spawn(move || serve_input(listener, input, input_state));
    fs::write(directory.join("started"), b"").context("record window startup")?;
    let stdout_thread = copy_output(
        stdout,
        std::io::stdout(),
        WindowOutputChannel::Stdout,
        Arc::clone(&event_file),
    );
    let stderr_thread = copy_output(
        stderr,
        std::io::stderr(),
        WindowOutputChannel::Stderr,
        event_file,
    );
    let status = child.wait().context("wait for managed executable")?;
    stdout_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stdout copier panicked"))??;
    stderr_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stderr copier panicked"))??;
    let stopped = directory.join("stopped").exists();
    write_status(
        &directory,
        &Status {
            state: if stopped {
                WindowState::Stopped
            } else {
                WindowState::Exited
            },
            exit_code: status.code(),
            exit_signal: status.signal(),
        },
    )
}

fn start(launch: Launch) -> Result<()> {
    launch.validate()?;
    let directory = window_dir(launch.window_id);
    fs::create_dir_all(&directory).context("create window state")?;
    let launch_path = directory.join("launch.json");
    if launch_path.exists() {
        let existing: Launch = serde_json::from_slice(&fs::read(&launch_path)?)?;
        if existing.window_id != launch.window_id
            || existing.argv != launch.argv
            || existing.cwd != launch.cwd
        {
            bail!("window ID already has different launch inputs");
        }
        let tmux_path = directory.join("tmux-window-id");
        if tmux_path.exists() {
            let tmux_window_id = read_trimmed(&tmux_path)?;
            let exists = Command::new("/usr/bin/tmux")
                .args([
                    "display-message",
                    "-p",
                    "-t",
                    &tmux_window_id,
                    "#{window_id}",
                ])
                .status()?
                .success();
            if exists {
                wait_started(&directory)?;
                return write_json(&Started { tmux_window_id });
            }
        }
    } else {
        write_json_file(&launch_path, &launch)?;
    }
    write_status(
        &directory,
        &Status {
            state: WindowState::Running,
            exit_code: None,
            exit_signal: None,
        },
    )?;
    ensure_session()?;
    if let Some(tmux_window_id) = recover_tmux_window(launch.window_id)? {
        fs::write(
            directory.join("tmux-window-id"),
            format!("{tmux_window_id}\n"),
        )?;
        wait_started(&directory)?;
        return write_json(&Started { tmux_window_id });
    }
    let short = launch.window_id.to_string();
    let command = format!("/usr/local/bin/wtg window-run {}", launch.window_id);
    let output = Command::new("/usr/bin/tmux")
        .args([
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{window_id}",
            "-t",
            "wt-host",
            "-n",
        ])
        .arg(format!("wt-{}", &short[..8]))
        .arg(command)
        .output()
        .context("create Byobu window")?;
    command_success("create Byobu window", &output)?;
    let tmux_window_id = String::from_utf8(output.stdout)?.trim().to_owned();
    if !valid_tmux_window_id(&tmux_window_id) {
        bail!("tmux returned invalid window ID {tmux_window_id:?}");
    }
    fs::write(
        directory.join("tmux-window-id"),
        format!("{tmux_window_id}\n"),
    )?;
    wait_started(&directory)?;
    write_json(&Started { tmux_window_id })
}

fn wait_started(directory: &Path) -> Result<()> {
    for _ in 0..100 {
        if directory.join("started").exists() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    bail!("managed executable did not start")
}

impl Launch {
    fn validate(&self) -> Result<()> {
        if self.argv.is_empty() || self.argv.len() > 256 {
            bail!("window argv must contain between 1 and 256 items");
        }
        if self.argv.iter().any(|value| value.as_bytes().contains(&0)) {
            bail!("window argv contains NUL");
        }
        if !self.cwd.starts_with('/') || self.cwd.as_bytes().contains(&0) {
            bail!("window cwd must be an absolute path without NUL");
        }
        Ok(())
    }
}

fn observe(request: ObserveRequest) -> Result<()> {
    let directory = window_dir(request.window_id);
    let tmux_window_id = read_trimmed(&directory.join("tmux-window-id"))?;
    let status: Status = serde_json::from_slice(&fs::read(directory.join("status.json"))?)?;
    let (output_offset, output) =
        read_output(&directory.join("output.bin"), request.output_offset)?;
    let capture = Command::new("/usr/bin/tmux")
        .args(["capture-pane", "-p", "-t", &tmux_window_id, "-S", "-200"])
        .output()
        .context("capture Byobu window")?;
    command_success("capture Byobu window", &capture)?;
    write_json(&Observation {
        tmux_window_id,
        state: status.state,
        exit_code: status.exit_code,
        exit_signal: status.exit_signal,
        output_offset,
        output,
        screen: String::from_utf8_lossy(&capture.stdout).into_owned(),
        screen_observed_at_unix_ms: now_unix_ms(),
    })
}

fn input(request: InputRequest) -> Result<()> {
    let mut stream = UnixStream::connect(window_dir(request.window_id).join("input.sock"))
        .context("connect managed window input")?;
    stream.write_all(&request.sequence_id.to_be_bytes())?;
    stream.write_all(&request.data)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut acknowledgment = [0u8; 1];
    stream.read_exact(&mut acknowledgment)?;
    if acknowledgment != [1] {
        bail!("managed window rejected input sequence");
    }
    write_json(&serde_json::json!({}))
}

fn serve_input(listener: UnixListener, mut child_input: impl Write, state_path: PathBuf) {
    let mut last_sequence = fs::read_to_string(&state_path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { break };
        let result = (|| -> std::io::Result<()> {
            let mut sequence = [0u8; 8];
            stream.read_exact(&mut sequence)?;
            let sequence = u64::from_be_bytes(sequence);
            let mut data = Vec::new();
            stream.read_to_end(&mut data)?;
            if sequence > last_sequence {
                if sequence != last_sequence + 1 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "window input sequence has a gap",
                    ));
                }
                child_input.write_all(&data)?;
                child_input.flush()?;
                fs::write(&state_path, format!("{sequence}\n"))?;
                last_sequence = sequence;
            }
            stream.write_all(&[1])
        })();
        if result.is_err() {
            let _ = stream.write_all(&[0]);
            break;
        }
    }
}

fn stop(request: Target) -> Result<()> {
    stop_directory(&window_dir(request.window_id))?;
    write_json(&serde_json::json!({}))
}

fn stop_all() -> Result<()> {
    stop_directories(Path::new(STATE_ROOT))?;
    write_json(&serde_json::json!({}))
}

fn stop_directories(root: &Path) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut first_error = None;
    for entry in entries {
        let result = (|| -> Result<()> {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                stop_directory(&entry.path())?;
            }
            Ok(())
        })();
        if result.is_err() && first_error.is_none() {
            first_error = result.err();
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn stop_directory(directory: &Path) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    fs::write(directory.join("stopped"), b"")?;
    let pid_path = directory.join("pid");
    if !pid_path.exists() {
        return Ok(());
    }
    let identity = read_trimmed(&pid_path)?;
    let (pid, start_time) = identity
        .split_once(' ')
        .context("invalid managed window process identity")?;
    let raw_pid: u32 = pid.parse()?;
    let start_time: u64 = start_time.parse()?;
    if let Some(current) = process_identity(raw_pid)? {
        if current.process_group != raw_pid || current.start_time != start_time {
            bail!("managed window process identity changed");
        }
    } else if !process_group_exists(nix::unistd::Pid::from_raw(raw_pid.try_into()?)) {
        return Ok(());
    }
    let pid = nix::unistd::Pid::from_raw(raw_pid.try_into()?);
    signal_process_group(pid, nix::sys::signal::Signal::SIGTERM)?;
    for _ in 0..20 {
        if !process_group_exists(pid) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    signal_process_group(pid, nix::sys::signal::Signal::SIGKILL)?;
    for _ in 0..20 {
        if !process_group_exists(pid) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    bail!("managed executable did not stop")
}

fn signal_process_group(pid: nix::unistd::Pid, signal: nix::sys::signal::Signal) -> Result<()> {
    match nix::sys::signal::killpg(pid, signal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn process_group_exists(pid: nix::unistd::Pid) -> bool {
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(-pid.as_raw()), None) {
        Ok(()) | Err(nix::errno::Errno::EPERM) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ProcessIdentity {
    process_group: u32,
    start_time: u64,
}

fn process_identity(pid: u32) -> Result<Option<ProcessIdentity>> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let fields = stat
        .rsplit_once(") ")
        .context("invalid process stat")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.len() < 20 {
        bail!("invalid process stat");
    }
    Ok(Some(ProcessIdentity {
        process_group: fields[2].parse()?,
        start_time: fields[19].parse()?,
    }))
}

fn delete(request: Target) -> Result<()> {
    let directory = window_dir(request.window_id);
    if !directory.exists() {
        return write_json(&serde_json::json!({}));
    }
    delete_directory(&directory, request.window_id)?;
    write_json(&serde_json::json!({}))
}

fn delete_directory(directory: &Path, window_id: WindowId) -> Result<()> {
    stop_directory(directory)?;
    let target = if directory.join("tmux-window-id").exists() {
        Some(read_trimmed(&directory.join("tmux-window-id"))?)
    } else {
        recover_tmux_window(window_id)?
    };
    if let Some(target) = target {
        let output = Command::new("/usr/bin/tmux")
            .args(["kill-window", "-t", &target])
            .output()?;
        if !output.status.success()
            && !String::from_utf8_lossy(&output.stderr).contains("can't find window")
        {
            command_success("delete Byobu window", &output)?;
        }
    }
    fs::remove_dir_all(directory)?;
    Ok(())
}

fn recover_tmux_window(window_id: WindowId) -> Result<Option<String>> {
    let output = Command::new("/usr/bin/tmux")
        .args([
            "list-windows",
            "-t",
            "wt-host",
            "-F",
            "#{window_id}\t#{pane_start_command}",
        ])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let expected = format!("/usr/local/bin/wtg window-run {window_id}");
    parse_tmux_windows(&String::from_utf8_lossy(&output.stdout), &expected)
}

fn parse_tmux_windows(output: &str, expected_command: &str) -> Result<Option<String>> {
    let matches = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, command)| *command == expected_command)
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [id] if valid_tmux_window_id(id) => Ok(Some((*id).to_owned())),
        [id] => bail!("tmux returned invalid window ID {id:?}"),
        _ => bail!("multiple Byobu windows have the same managed window identity"),
    }
}

fn ensure_session() -> Result<()> {
    if Command::new("/usr/bin/tmux")
        .args(["has-session", "-t", "wt-host"])
        .status()?
        .success()
    {
        return Ok(());
    }
    let output = Command::new("/usr/bin/tmux")
        .args(["new-session", "-d", "-s", "wt-host"])
        .output()?;
    command_success("create Byobu tmux session", &output)
}

fn read_output(path: &Path, offset: u64) -> Result<(u64, Vec<OutputChunk>)> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, vec![])),
        Err(error) => return Err(error.into()),
    };
    let length = file.metadata()?.len();
    if offset > length {
        bail!("output cursor exceeds guest output length");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut consumed = offset;
    let mut records = Vec::new();
    let mut total = 0;
    while consumed < length && total < MAX_OBSERVATION_BYTES {
        let mut header = [0u8; 9];
        file.read_exact(&mut header)?;
        let size = u64::from_be_bytes(header[1..].try_into().unwrap());
        let size: usize = size.try_into().context("output record is too large")?;
        if size > MAX_OBSERVATION_BYTES {
            bail!("output record exceeds observation limit");
        }
        let mut data = vec![0; size];
        file.read_exact(&mut data)?;
        consumed += 9 + size as u64;
        total += size;
        let channel = match header[0] {
            1 => WindowOutputChannel::Stdout,
            2 => WindowOutputChannel::Stderr,
            _ => bail!("invalid output channel marker"),
        };
        records.push(OutputChunk { channel, data });
    }
    Ok((consumed, records))
}

fn copy_output(
    mut source: impl Read + Send + 'static,
    mut display: impl Write + Send + 'static,
    channel: WindowOutputChannel,
    events: Arc<Mutex<File>>,
) -> std::thread::JoinHandle<Result<()>> {
    std::thread::spawn(move || {
        let marker = if channel == WindowOutputChannel::Stdout {
            1
        } else {
            2
        };
        let mut buffer = vec![0; 16 * 1024];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            {
                let mut events = events
                    .lock()
                    .map_err(|_| anyhow::anyhow!("output log lock poisoned"))?;
                events.write_all(&[marker])?;
                events.write_all(&(count as u64).to_be_bytes())?;
                events.write_all(&buffer[..count])?;
                events.flush()?;
            }
            display.write_all(&buffer[..count])?;
            display.flush()?;
        }
        Ok(())
    })
}

fn window_dir(window_id: WindowId) -> PathBuf {
    Path::new(STATE_ROOT).join(window_id.to_string())
}
fn read_json<T: for<'de> Deserialize<'de>>() -> Result<T> {
    serde_json::from_reader(std::io::stdin().lock()).context("parse window request")
}
fn write_json(value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(std::io::stdout().lock(), value).context("write window response")
}
fn write_json_file(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec(value)?).with_context(|| format!("write {}", path.display()))
}
fn write_status(directory: &Path, value: &Status) -> Result<()> {
    write_json_file(&directory.join("status.json"), value)
}
fn read_trimmed(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}
fn valid_tmux_window_id(value: &str) -> bool {
    value
        .strip_prefix('@')
        .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
}
fn command_success(context: &str, output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "window_guest/tests.rs"]
mod tests;
