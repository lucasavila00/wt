use super::*;

pub(super) struct ConsoleLog {
    file: fs::File,
    pending_line: Vec<u8>,
}

impl ConsoleLog {
    pub(super) fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            file: fs::File::open(path).context("open image build console log")?,
            pending_line: Vec::new(),
        })
    }

    pub(super) fn drain(&mut self) -> Result<Vec<String>> {
        let mut bytes = Vec::new();
        self.file
            .read_to_end(&mut bytes)
            .context("read image build console log")?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(extract_phase_markers(&mut self.pending_line, &bytes))
    }
}

pub(super) fn extract_phase_markers(pending_line: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    const PREFIX: &str = "WT_IMAGE_PHASE=";

    pending_line.extend_from_slice(bytes);
    let mut phases = Vec::new();
    let mut consumed = 0;
    for (index, byte) in pending_line.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let line = String::from_utf8_lossy(&pending_line[consumed..index]);
        if let Some(phase) = line.strip_prefix(PREFIX) {
            phases.push(phase.trim_end_matches('\r').to_owned());
        }
        consumed = index + 1;
    }
    pending_line.drain(..consumed);
    phases
}

pub(super) fn progress_message(phase: &str, elapsed: Duration) -> String {
    format!("Image build: {phase} (elapsed={}s)", elapsed.as_secs())
}

fn drain_console(console: &mut ConsoleLog, started: Instant) -> Result<Option<String>> {
    let phases = console.drain()?;
    let mut last_phase = None;
    for phase in phases {
        println!("{}", progress_message(&phase, started.elapsed()));
        last_phase = Some(phase);
    }
    Ok(last_phase)
}

pub(super) fn wait_for_shutdown(
    runner: &impl Runner,
    console: &mut ConsoleLog,
    domain_name: &str,
) -> Result<()> {
    let started = Instant::now();
    let deadline = Instant::now() + IMAGE_BUILD_TIMEOUT;
    let mut next_state_check = Instant::now();
    let mut next_heartbeat = Instant::now() + Duration::from_secs(60);
    let mut phase = String::from("starting cloud-init");
    loop {
        if let Some(next_phase) = drain_console(console, started)? {
            phase = next_phase;
            next_heartbeat = Instant::now() + Duration::from_secs(60);
        }

        let now = Instant::now();
        if now >= next_state_check {
            let state = runner.text(
                cmd!("virsh", "-c", LIBVIRT_URI, "domstate", domain_name),
                "read image build domain state",
            )?;
            if state.trim() == "shut off" {
                drain_console(console, started)?;
                println!("Guest powered off after {}s.", started.elapsed().as_secs());
                return Ok(());
            }
            next_state_check = now + Duration::from_secs(3);
        }

        if now >= next_heartbeat {
            println!("{}", progress_message(&phase, started.elapsed()));
            next_heartbeat = now + Duration::from_secs(60);
        }
        if now >= deadline {
            bail!("timed out waiting for KVM image build guest; last phase: {phase}");
        }
        thread::sleep(Duration::from_millis(250));
    }
}
