use std::fs;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};
use wt_end_to_end_tests::cmd;

pub(crate) fn spawn_gateway(temp: &Path, binary_dir: &Path) -> Child {
    let control_socket = temp.join("gateway-control.sock");
    let transport_socket = temp.join("gateway-transport.sock");
    let log_path = temp.join("gateway.log");
    for socket in [&control_socket, &transport_socket] {
        match fs::remove_file(socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove stale gateway socket: {error}"),
        }
    }
    let mut gateway = cmd!(
        binary_dir.join("wt-agent-tool-gateway"),
        "--control-socket",
        &control_socket,
        "--transport-socket",
        &transport_socket,
        "--state-file",
        temp.join("gateway-state.json"),
        "--local-provider",
        format!("local.test={}", temp.display()),
    );
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap();
    let mut gateway = gateway
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    for socket in [&control_socket, &transport_socket] {
        while !socket.exists() {
            assert_gateway_running(&mut gateway, &log_path);
            assert!(Instant::now() < deadline, "gateway socket did not appear");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_gateway_running(&mut gateway, &log_path);
    gateway
}

fn assert_gateway_running(gateway: &mut Child, log_path: &Path) {
    let Some(status) = gateway.try_wait().unwrap() else {
        return;
    };
    let log = fs::read_to_string(log_path).unwrap_or_else(|error| error.to_string());
    panic!(
        "test gateway exited during startup ({status})\ngateway log ({}):\n{log}",
        log_path.display()
    );
}
