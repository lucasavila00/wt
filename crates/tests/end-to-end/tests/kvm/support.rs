use super::binaries::prepare_test_binaries;
use super::fixture::*;
use super::gateway::spawn_gateway;
use super::images::{isolated_test_images, unique_vsock_port};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, TransportRequest, TransportResponse,
    PROTOCOL_VERSION, VSOCK_PORT_ENV,
};
use wt_control_protocol::{
    ApiProgress, ApiRequest, ApiResponse, CreateInstance, InstanceName, Operation, Outcome,
    Response,
};
use wt_end_to_end_tests::cmd;
use wt_server::ServerConfig;
use wt_workload_registry::{CapacityConfig, Resources};

pub(crate) static KVM_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct KvmHarness {
    pub(crate) git: GitFixture,
    pub(crate) gateway: Child,
    pub(crate) temp: TempDir,
    pub(crate) config: ServerConfig,
    pub(crate) server_config_path: PathBuf,
    pub(crate) wt_binary: PathBuf,
    pub(crate) guest_public_key: String,
    pub(crate) initial_disks: usize,
    _images: TempDir,
}

impl KvmHarness {
    pub(crate) fn new(timings: &mut Timings) -> Self {
        let temp = TempDir::new().unwrap();
        let vsock_port = unique_vsock_port();
        std::env::set_var(VSOCK_PORT_ENV, vsock_port.to_string());
        let workspace =
            fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap();
        let binary_dir = temp.path().join("bin");
        timings.run("build isolated test binaries", || {
            prepare_test_binaries(&workspace, &binary_dir)
        });
        let mut config = match std::env::var_os("WT_KVM_SERVER_CONFIG") {
            Some(path) => ServerConfig::load_runtime_from(Path::new(&path)).unwrap(),
            None => ServerConfig::load_runtime_from(
                &workspace.join("examples/server-config/wt-server.kvm-test.toml"),
            )
            .unwrap(),
        };
        config.test_server = true;
        let wt_binary = config.install.binary_dir.join("wt");
        assert_eq!(config.agent_tools.vsock_port, vsock_port);
        config.agent_tools.github.as_mut().unwrap().host = "local.test".to_owned();
        let installed_image = config.image.path.clone();
        let images = timings.run("prepare isolated golden images", || {
            isolated_test_images(&workspace, &installed_image, &binary_dir)
        });
        config.image.path = images.path().join("retained.qcow2");
        config.install.binary_dir = binary_dir;
        let initial_disks = count_disks(&config.libvirt.worlds_dir);
        let git = timings.run("prepare local Git fixture", || {
            GitFixture::create(temp.path())
        });
        let guest_public_key = fs::read_to_string(&git.guest_public_key)
            .unwrap()
            .trim()
            .to_owned();
        std::env::set_var("HOME", temp.path());
        fs::create_dir_all(temp.path().join(".ssh")).unwrap();
        fs::write(
            temp.path().join(".ssh/known_hosts"),
            "local.test ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".ssh/config"),
            format!(
                "Include ~/.ssh/wt/config\nHost *\n  IdentityFile {}\n  IdentitiesOnly yes\n",
                git.guest_key.display(),
            ),
        )
        .unwrap();
        let server_config_path = temp.path().join("server.toml");
        fs::write(&server_config_path, toml::to_string(&config).unwrap()).unwrap();
        let capacity = CapacityConfig {
            version: 1,
            limits: Resources {
                vcpus: 16,
                memory_mib: 32_768,
                disk_gib: 1_024,
            },
        };
        fs::write(
            temp.path().join("capacity.toml"),
            toml::to_string(&capacity).unwrap(),
        )
        .unwrap();
        let gateway = spawn_gateway(temp.path(), &config.install.binary_dir);
        let control_socket = temp.path().join("gateway-control.sock");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !control_socket.exists() {
            assert!(
                Instant::now() < deadline,
                "gateway control socket did not appear"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        Self {
            git,
            gateway,
            temp,
            config,
            server_config_path,
            wt_binary,
            guest_public_key,
            initial_disks,
            _images: images,
        }
    }

    pub(crate) fn create(&self, name: &InstanceName) -> wt_control_protocol::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Create(CreateInstance {
                name: name.clone(),
                vcpus: 2,
                memory_mib: 4096,
                disk_gib: 32,
                ssh_authorized_keys: vec![self.guest_public_key.clone()],
                git_user_name: "WT E2E".to_owned(),
                git_user_email: "wt@example.invalid".to_owned(),
            }),
        ) else {
            panic!("expected instance response");
        };
        *instance
    }

    pub(crate) fn sync_inventory(&self) -> Vec<wt_control_protocol::Instance> {
        let Response::Instances { instances, .. } =
            call_api(self.temp.path(), &self.server_config_path, Operation::List)
        else {
            panic!("expected list response");
        };
        sync_inventory(&instances).unwrap();
        instances
    }

    pub(crate) fn delete(&self, name: &InstanceName) {
        call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Delete { name: name.clone() },
        );
    }

    pub(crate) fn shutdown(&self, name: &InstanceName) -> wt_control_protocol::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Stop { name: name.clone() },
        ) else {
            panic!("expected instance response");
        };
        *instance
    }

    pub(crate) fn start(&self, name: &InstanceName) -> wt_control_protocol::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Start { name: name.clone() },
        ) else {
            panic!("expected instance response");
        };
        *instance
    }

    pub(crate) fn restart_gateway(&mut self) {
        self.gateway.kill().unwrap();
        self.gateway.wait().unwrap();
        self.gateway = spawn_gateway(self.temp.path(), &self.config.install.binary_dir);
    }

    pub(crate) fn grant_token_for(&self, world_id: uuid::Uuid) -> String {
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(self.temp.path().join("gateway-state.json")).unwrap())
                .unwrap();
        state["grants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|grant| grant["world_id"] == world_id.to_string())
            .and_then(|grant| grant["token"].as_str())
            .unwrap()
            .to_owned()
    }

    pub(crate) fn assert_grant_is_revoked(&self, token: String) {
        let mut stream = std::os::unix::net::UnixStream::connect(
            self.temp.path().join("gateway-transport.sock"),
        )
        .unwrap();
        write_json_line(
            &mut stream,
            &TransportRequest {
                protocol_version: PROTOCOL_VERSION,
                token,
                operation: ClientOperation::Cli { args: Vec::new() },
            },
        )
        .unwrap();
        let response: TransportResponse = read_json_line(&mut stream).unwrap();
        assert!(!response.ok, "deleted world grant still works");
    }
}

impl Drop for KvmHarness {
    fn drop(&mut self) {
        let worlds =
            match call_api_result(self.temp.path(), &self.server_config_path, Operation::List) {
                Ok(Response::Instances { instances, .. }) => instances,
                Ok(response) => {
                    eprintln!("KVM cleanup: unexpected list response: {response:?}");
                    return;
                }
                Err(error) => {
                    eprintln!("KVM cleanup: list worlds: {error}");
                    return;
                }
            };
        for world in worlds.into_iter().rev() {
            if let Err(error) = call_api_result(
                self.temp.path(),
                &self.server_config_path,
                Operation::Delete {
                    name: world.name.clone(),
                },
            ) {
                eprintln!("KVM cleanup: delete {}: {error}", world.name);
            }
        }
        let remaining = count_disks(&self.config.libvirt.worlds_dir);
        if remaining != self.initial_disks {
            eprintln!(
                "KVM cleanup: disk-node count is {remaining}, expected {}",
                self.initial_disks
            );
        }
        let _ = self.gateway.kill();
        let _ = self.gateway.wait();
    }
}

pub(crate) fn unique_name(label: &str) -> InstanceName {
    InstanceName::parse(format!(
        "e15-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
    .unwrap()
}

pub(crate) fn run_guest(harness: &KvmHarness, name: &InstanceName, command: &str, action: &str) {
    let output = guest_command(harness, name, command).output().unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn guest_command(
    harness: &KvmHarness,
    name: &InstanceName,
    command: &str,
) -> std::process::Command {
    cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-direct"),
        command,
    )
}

pub(crate) fn count_disks(worlds_dir: &Path) -> usize {
    let entries = match fs::read_dir(worlds_dir.join("disks")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(error) => panic!("read disk node directory: {error}"),
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("qcow2"))
        .count()
}

pub(crate) fn sync_inventory(instances: &[wt_control_protocol::Instance]) -> Result<(), String> {
    let client_config = wt_client::config::ClientConfig {
        contexts: vec![wt_client::config::Context {
            name: "local".into(),
            kind: wt_client::config::ContextKind::BareMetalLocal,
        }],
    };
    wt_client::ssh::sync(
        &client_config,
        &instances
            .iter()
            .cloned()
            .map(|instance| wt_client::inventory::ContextInstance {
                context: "local".into(),
                agent_tool_report_count: 0,
                disk_usage_bytes: None,
                instance,
            })
            .collect::<Vec<_>>(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub(crate) fn call_api(home: &Path, config: &Path, operation: Operation) -> Response {
    call_api_result(home, config, operation).unwrap()
}

pub(crate) fn call_api_result(
    home: &Path,
    config: &Path,
    operation: Operation,
) -> Result<Response, String> {
    // Match the restrictive umask of the installed wt-server.service. QEMU must
    // still be able to traverse the world directory and open its disk images.
    let mut child = cmd!(
        "/bin/sh",
        "-c",
        "umask 077; exec \"$@\"",
        "sh",
        env!("CARGO_BIN_EXE_wt-test-server"),
        "--config",
        config,
        "--capacity",
        home.join("capacity.toml"),
        "api",
    )
    .env("HOME", home)
    .env(
        "WT_AGENT_TOOL_TEST_CONTROL_SOCKET",
        home.join("gateway-control.sock"),
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .map_err(|error| error.to_string())?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .ok_or("test server stdin unavailable")?,
        &ApiRequest::new(operation),
    )
    .map_err(|error| error.to_string())?;
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    ensure_success("call test server API", &output)?;
    let mut response = None;
    for line in output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if serde_json::from_slice::<ApiProgress>(line).is_ok() {
            if response.is_some() {
                return Err("test server returned progress after its final response".into());
            }
            continue;
        }
        let frame =
            serde_json::from_slice::<ApiResponse>(line).map_err(|error| error.to_string())?;
        if response.replace(frame).is_some() {
            return Err("test server returned multiple API responses".into());
        }
    }
    let response = response.ok_or("test server returned no API response")?;
    match response.outcome {
        Outcome::Ok { response } => Ok(*response),
        Outcome::Error { error } => Err(format!("{}: {}", error.code as u8, error.message)),
    }
}

pub(crate) struct Timings {
    pub(crate) started: Instant,
    pub(crate) phases: Vec<(&'static str, Duration)>,
}

impl Timings {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            phases: Vec::new(),
        }
    }

    pub(crate) fn run<T>(&mut self, label: &'static str, action: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = action();
        self.phases.push((label, started.elapsed()));
        result
    }
}

impl Drop for Timings {
    fn drop(&mut self) {
        eprintln!("KVM E2E timings:");
        for (label, elapsed) in &self.phases {
            eprintln!("  {label}: {:.1}s", elapsed.as_secs_f64());
        }
        eprintln!("  total: {:.1}s", self.started.elapsed().as_secs_f64());
    }
}
