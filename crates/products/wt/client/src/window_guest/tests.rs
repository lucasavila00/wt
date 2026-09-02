use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct SharedBytes(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn input_socket_preserves_nul_and_non_line_bytes_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("input.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let output = SharedBytes(Arc::clone(&bytes));
    let state = temp.path().join("sequence");
    let thread = std::thread::spawn(move || serve_input(listener, output, state));
    let expected = b"not-a-line\0\x1b[31m";
    let mut stream = UnixStream::connect(path).unwrap();
    stream.write_all(&1u64.to_be_bytes()).unwrap();
    stream.write_all(expected).unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut acknowledgment = [0];
    stream.read_exact(&mut acknowledgment).unwrap();
    assert_eq!(acknowledgment, [1]);
    drop(stream);
    let path = temp.path().join("input.sock");
    let mut replay = UnixStream::connect(path).unwrap();
    replay.write_all(&1u64.to_be_bytes()).unwrap();
    replay.write_all(expected).unwrap();
    replay.shutdown(std::net::Shutdown::Write).unwrap();
    replay.read_exact(&mut acknowledgment).unwrap();
    assert_eq!(acknowledgment, [1]);
    drop(replay);
    let bad = UnixStream::connect(temp.path().join("input.sock")).unwrap();
    bad.shutdown(std::net::Shutdown::Write).unwrap();
    drop(bad);
    drop(temp);
    thread.join().unwrap();
    assert_eq!(&*bytes.lock().unwrap(), expected);
}

#[test]
fn stop_kills_descendants_after_the_process_group_leader_exits() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new("/bin/sh")
        .args(["-c", "sleep 10 & sleep 0.2"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    let identity = process_identity(pid).unwrap().unwrap();
    child.wait().unwrap();
    let group = nix::unistd::Pid::from_raw(pid.try_into().unwrap());
    assert!(process_group_exists(group));
    write_status(
        temp.path(),
        &Status {
            state: WindowState::Exited,
            exit_code: Some(0),
            exit_signal: None,
        },
    )
    .unwrap();
    fs::write(
        temp.path().join("pid"),
        format!("{pid} {}\n", identity.start_time),
    )
    .unwrap();

    stop_directory(temp.path()).unwrap();
    assert!(!process_group_exists(group));
}

#[test]
fn delete_preserves_state_when_stopping_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_status(
        temp.path(),
        &Status {
            state: WindowState::Running,
            exit_code: None,
            exit_signal: None,
        },
    )
    .unwrap();
    fs::write(temp.path().join("pid"), "not a process identity").unwrap();

    assert!(delete_directory(temp.path(), WindowId::new()).is_err());
    assert!(temp.path().exists());
}

#[test]
fn stop_all_propagates_a_window_stop_failure() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("window");
    fs::create_dir(&directory).unwrap();
    write_status(
        &directory,
        &Status {
            state: WindowState::Running,
            exit_code: None,
            exit_signal: None,
        },
    )
    .unwrap();
    fs::write(directory.join("pid"), "not a process identity").unwrap();

    assert!(stop_directories(temp.path()).is_err());
}

#[test]
fn process_identity_reads_the_kernel_start_time() {
    let identity = process_identity(std::process::id()).unwrap().unwrap();
    assert!(identity.process_group > 0);
    assert!(identity.start_time > 0);
}

#[test]
fn recovers_only_the_exact_managed_start_command() {
    assert_eq!(
        parse_tmux_windows(
            "@3\t/bin/sh\n@7\t/usr/local/bin/wtg window-run abc\n",
            "/usr/local/bin/wtg window-run abc",
        )
        .unwrap(),
        Some("@7".into())
    );
    assert!(parse_tmux_windows(
        "@7\t/usr/local/bin/wtg window-run abc\n@8\t/usr/local/bin/wtg window-run abc\n",
        "/usr/local/bin/wtg window-run abc",
    )
    .is_err());
}
