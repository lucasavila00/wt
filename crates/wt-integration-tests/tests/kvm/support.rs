use super::fixture::*;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{
    ApiRequest, ApiResponse, CreateApplication, CreateInstance, InstanceName, InstanceStatus,
    Operation, Outcome, Response,
};
use wt_command::cmd;
use wt_devcontainer_git::{
    read_json_line, write_json_line, ClientOperation, ControlRequest, ControlResponse,
    TransportRequest, TransportResponse, PROTOCOL_VERSION,
};
use wt_server::ServerConfig;

pub(crate) static KVM_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct KvmHarness {
    pub(crate) git: GitFixture,
    pub(crate) gateway: Child,
    pub(crate) temp: TempDir,
    pub(crate) config: ServerConfig,
    pub(crate) server_config_path: PathBuf,
    pub(crate) guest_public_key: String,
    pub(crate) initial_disk_nodes: usize,
    api_fixture: Option<JoinHandle<Result<(), String>>>,
}

impl KvmHarness {
    pub(crate) fn new(timings: &mut Timings) -> Self {
        let temp = TempDir::new().unwrap();
        let workspace =
            fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        timings.run("build guest helpers", || {
            let mut command = cmd!(
                env!("CARGO"),
                "build",
                "-p",
                "wt-devcontainer-guest",
                "-p",
                "wt-devcontainer-git",
            );
            command.current_dir(&workspace);
            run(command, "build guest helpers")
        });
        let mut config = match std::env::var_os("WT_KVM_SERVER_CONFIG") {
            Some(path) => ServerConfig::load_from(Path::new(&path)).unwrap(),
            None => ServerConfig::load_from(
                &workspace.join("examples/server-config/wt-server.kvm-test.toml"),
            )
            .unwrap(),
        };
        config.install.binary_dir = workspace.join("target/debug");
        let initial_disk_nodes = count_disk_nodes(&config.libvirt.worlds_dir);
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
                "Include {}\nHost *\n  IdentityFile {}\n  IdentitiesOnly yes\n",
                temp.path().join(".ssh/wt/config").display(),
                git.guest_key.display(),
            ),
        )
        .unwrap();
        let server_config_path = temp.path().join("server.toml");
        fs::write(&server_config_path, toml::to_string(&config).unwrap()).unwrap();
        let gateway = spawn_gateway(temp.path(), &config.install.binary_dir, None);
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
            guest_public_key,
            initial_disk_nodes,
            api_fixture: None,
        }
    }

    pub(crate) fn create(&self, name: &InstanceName) -> wt_api::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Create(CreateInstance {
                name: name.clone(),
                vcpus: 1,
                memory_mib: 1024,
                disk_gib: 32,
                ssh_authorized_keys: vec![self.guest_public_key.clone()],
                application: CreateApplication::Devcontainer {
                    source: self.git.url(),
                    git_base: "main".into(),
                    git_user_name: "WT E2E".to_owned(),
                    git_user_email: "wt@example.invalid".to_owned(),
                },
            }),
        ) else {
            panic!("expected instance response");
        };
        *instance
    }

    pub(crate) fn sync_inventory(&self) -> Vec<wt_api::Instance> {
        let Response::Instances { instances } =
            call_api(self.temp.path(), &self.server_config_path, Operation::List)
        else {
            panic!("expected list response");
        };
        sync_inventory(&instances).unwrap();
        instances
    }

    pub(crate) fn finish_setup(&self, name: &InstanceName) -> wt_api::Instance {
        self.sync_inventory();
        let mut setup = start_world_setup(self.temp.path(), name);
        let instance =
            wait_for_running(self.temp.path(), &self.server_config_path, name, &mut setup);
        let _ = setup.kill();
        let _ = setup.wait();
        instance
    }

    pub(crate) fn delete(&self, name: &InstanceName) {
        call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Delete { name: name.clone() },
        );
    }

    pub(crate) fn stop(&self, instance: &wt_api::Instance) {
        run(
            cmd!(
                "virsh",
                "--connect",
                "qemu:///system",
                "destroy",
                format!("wt-{}", instance.id.simple()),
            ),
            "stop KVM world",
        );
    }

    pub(crate) fn start(&self, name: &InstanceName) -> wt_api::Instance {
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
        self.gateway = spawn_gateway(self.temp.path(), &self.config.install.binary_dir, None);
        if let Some(fixture) = self.api_fixture.take() {
            fixture.join().unwrap().unwrap();
        }
    }

    pub(crate) fn use_provider_api_fixture(&mut self, kind: &str, head: &str) {
        self.gateway.kill().unwrap();
        self.gateway.wait().unwrap();
        let (base_url, fixture) = spawn_provider_api_fixture(kind, head);
        let token_file = self.temp.path().join("provider-api-token");
        fs::write(&token_file, "fixture-token\n").unwrap();
        self.gateway = spawn_gateway(
            self.temp.path(),
            &self.config.install.binary_dir,
            Some((kind, &base_url, &token_file)),
        );
        self.api_fixture = Some(fixture);
    }

    pub(crate) fn assert_shared_prefix_is_available(&self) {
        let mut stream =
            std::os::unix::net::UnixStream::connect(self.temp.path().join("gateway-control.sock"))
                .unwrap();
        write_json_line(
            &mut stream,
            &ControlRequest::Reserve {
                world_id: "different-world".to_owned(),
                source: self.git.url(),
                base: "main".to_owned(),
            },
        )
        .unwrap();
        let response: ControlResponse = read_json_line(&mut stream).unwrap();
        assert!(response.ok, "another world could not share the WT prefix");
    }

    pub(crate) fn grant_token(&self) -> String {
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(self.temp.path().join("gateway-state.json")).unwrap())
                .unwrap();
        state["grants"][0]["token"].as_str().unwrap().to_owned()
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
                operation: ClientOperation::Cli {
                    args: Vec::new(),
                    branch: None,
                    head: None,
                },
            },
        )
        .unwrap();
        let response: TransportResponse = read_json_line(&mut stream).unwrap();
        assert!(!response.ok, "deleted world grant still works");
    }
}

fn spawn_gateway(temp: &Path, binary_dir: &Path, api: Option<(&str, &str, &Path)>) -> Child {
    let control_socket = temp.join("gateway-control.sock");
    let transport_socket = temp.join("gateway-transport.sock");
    for socket in [&control_socket, &transport_socket] {
        match fs::remove_file(socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove stale gateway socket: {error}"),
        }
    }
    let mut gateway = cmd!(
        binary_dir.join("wt-agent-git-gateway"),
        "serve",
        "--control-socket",
        &control_socket,
        "--transport-socket",
        &transport_socket,
        "--state-file",
        temp.join("gateway-state.json"),
        "--local-provider",
        format!("local.test={}", temp.display()),
    );
    gateway.stdout(Stdio::null());
    if let Some((kind, base_url, token_file)) = api {
        gateway
            .env("WT_AGENT_GIT_TEST_PROVIDER_KIND", kind)
            .env("WT_AGENT_GIT_TEST_API_BASE", base_url)
            .env("WT_AGENT_GIT_TEST_TOKEN_FILE", token_file);
    }
    let gateway = gateway.stderr(Stdio::inherit()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    for socket in [&control_socket, &transport_socket] {
        while !socket.exists() {
            assert!(Instant::now() < deadline, "gateway socket did not appear");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    gateway
}

fn spawn_provider_api_fixture(kind: &str, head: &str) -> (String, JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let kind = kind.to_owned();
    let head = head.to_owned();
    let fixture = std::thread::spawn(move || {
        let (stream, _) = listener
            .accept()
            .map_err(|error| format!("accept provider fixture request: {error}"))?;
        serve_provider_api_request(stream, &kind, &head)?;
        Ok(())
    });
    (base_url, fixture)
}

fn serve_provider_api_request(mut stream: TcpStream, kind: &str, head: &str) -> Result<(), String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("clone provider fixture stream: {error}"))?,
    );
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| format!("read provider fixture request: {error}"))?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("provider fixture request has no path")?
        .to_owned();
    let mut content_length = 0;
    let mut authenticated = false;
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|error| format!("read provider fixture header: {error}"))?;
        if header == "\r\n" || header.is_empty() {
            break;
        }
        let lowercase = header.to_ascii_lowercase();
        if let Some(value) = lowercase.strip_prefix("content-length:") {
            content_length = value
                .trim()
                .parse::<usize>()
                .map_err(|error| format!("parse provider fixture content length: {error}"))?;
        }
        authenticated |= if kind == "github" {
            lowercase.trim() == "authorization: bearer fixture-token"
        } else {
            lowercase.trim() == "private-token: fixture-token"
        };
    }
    if !authenticated {
        return Err("provider fixture request was not authenticated".to_owned());
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("read provider fixture body: {error}"))?;
    let response = match (kind, path.as_str()) {
        ("github", "/graphql") => r#"{"data":{"viewer":{"login":"agent"},"repository":{"id":"repository-1","nameWithOwner":"acme/widget","viewerPermission":"WRITE","pullRequests":{"pageInfo":{"hasNextPage":false},"totalCount":0,"nodes":[]}}}}"#.to_owned(),
        ("github", path) if path.starts_with("/repos/acme/widget/actions/runs?") => {
            r#"{"total_count":0,"workflow_runs":[]}"#.to_owned()
        }
        ("gitlab", "/api/graphql") => format!(
            r#"{{"data":{{"currentUser":{{"username":"agent"}},"project":{{"id":"project-1","fullPath":"acme/widget","userPermissions":{{"createMergeRequestIn":true}},"repository":{{"commit":{{"sha":"{head}"}}}},"mergeRequests":{{"pageInfo":{{"hasNextPage":false}},"nodes":[]}}}}}}}}"#
        ),
        ("gitlab", path) if path.starts_with("/api/v4/projects/acme%2Fwidget/pipelines?") => {
            "[]".to_owned()
        }
        _ => return Err(format!("unexpected {kind} fixture request path: {path}")),
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write provider fixture response: {error}"))
}

impl Drop for KvmHarness {
    fn drop(&mut self) {
        let worlds =
            match call_api_result(self.temp.path(), &self.server_config_path, Operation::List) {
                Ok(Response::Instances { instances }) => instances,
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
        let remaining = count_disk_nodes(&self.config.libvirt.worlds_dir);
        if remaining != self.initial_disk_nodes {
            eprintln!(
                "KVM cleanup: disk-node count is {remaining}, expected {}",
                self.initial_disk_nodes
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
    let output = cmd!(
        "ssh",
        "-F",
        harness.temp.path().join(".ssh/config"),
        "-i",
        &harness.git.guest_key,
        format!("local.{name}-host"),
        command,
    )
    .output()
    .unwrap();
    ensure_success(action, &output).unwrap();
}

pub(crate) fn wait_for_running(
    home: &Path,
    config: &Path,
    name: &InstanceName,
    setup: &mut Child,
) -> wt_api::Instance {
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        let Response::Instance { instance } =
            call_api(home, config, Operation::Get { name: name.clone() })
        else {
            panic!("expected instance response")
        };
        if instance.status == InstanceStatus::Running {
            return *instance;
        }
        assert_ne!(
            instance.status,
            InstanceStatus::Error,
            "setup failed: {instance:?}"
        );
        if let Some(status) = setup.try_wait().unwrap() {
            let log = guest_setup_log(home, name);
            panic!("world setup SSH exited before completion: {status}\n{log}");
        }
        assert!(Instant::now() < deadline, "timed out waiting for setup");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn guest_setup_log(home: &Path, name: &InstanceName) -> String {
    let output = cmd!(
        "ssh",
        "-F",
        home.join(".ssh/config"),
        format!("local.{name}-host"),
        "tail -n 200 /var/lib/wt-setup/install.log",
    )
    .output();
    match output {
        Ok(output) => format!(
            "guest setup log:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => format!("could not read guest setup log: {error}"),
    }
}

pub(crate) fn count_disk_nodes(worlds_dir: &Path) -> usize {
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

pub(crate) fn sync_inventory(instances: &[wt_api::Instance]) -> Result<(), String> {
    let client_config = wt_cli::config::ClientConfig {
        contexts: vec![wt_cli::config::Context {
            name: "local".into(),
            kind: wt_cli::config::ContextKind::BareMetalLocal,
        }],
    };
    wt_cli::ssh::sync(
        &client_config,
        &instances
            .iter()
            .cloned()
            .map(|instance| wt_cli::inventory::ContextInstance {
                context: "local".into(),
                instance,
            })
            .collect::<Vec<_>>(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub(crate) fn start_world_setup(home: &Path, name: &InstanceName) -> Child {
    cmd!(
        "ssh",
        "-F",
        home.join(".ssh/config"),
        format!("local.{name}")
    )
    .env_remove("SSH_AUTH_SOCK")
    .env("TERM", "xterm-ghostty")
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()
    .expect("start first-SSH world setup")
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
        "api",
    )
    .env("HOME", home)
    .env(
        "WT_AGENT_GIT_TEST_CONTROL_SOCKET",
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
    let response: ApiResponse =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
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
