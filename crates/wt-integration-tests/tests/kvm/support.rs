use super::fixture::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wt_api::{
    ApiRequest, ApiResponse, CreateInstance, ForkInstance, InstanceName, InstanceStatus, Operation,
    Outcome, Response,
};
use wt_command::cmd;
use wt_server::ServerConfig;

pub(crate) static KVM_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct KvmHarness {
    pub(crate) git: GitSshServer,
    pub(crate) temp: TempDir,
    pub(crate) config: ServerConfig,
    pub(crate) server_config_path: PathBuf,
    pub(crate) guest_public_key: String,
    pub(crate) initial_disk_nodes: usize,
}

impl KvmHarness {
    pub(crate) fn new(timings: &mut Timings) -> Self {
        let temp = TempDir::new().unwrap();
        let workspace =
            fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        timings.run("build guest helpers", || {
            let mut command = cmd!(env!("CARGO"), "build", "-p", "wt-guest");
            command.current_dir(&workspace);
            run(command, "build guest helpers")
        });
        let mut config = match std::env::var_os("WT_KVM_SERVER_CONFIG") {
            Some(path) => ServerConfig::load_from(Path::new(&path)).unwrap(),
            None => ServerConfig::load().unwrap(),
        };
        config.install.binary_dir = workspace.join("target/debug");
        let initial_disk_nodes = count_disk_nodes(&config.libvirt.worlds_dir);
        let bridge_ip = network_address(&config.libvirt.network);
        let git = timings.run("prepare SSH Git fixture", || {
            GitSshServer::start(temp.path(), bridge_ip)
        });
        config.git.known_hosts_file = temp.path().join(".ssh/known_hosts");
        let guest_public_key = fs::read_to_string(&git.guest_public_key)
            .unwrap()
            .trim()
            .to_owned();
        std::env::set_var("HOME", temp.path());
        fs::create_dir_all(temp.path().join(".ssh")).unwrap();
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
        Self {
            git,
            temp,
            config,
            server_config_path,
            guest_public_key,
            initial_disk_nodes,
        }
    }

    pub(crate) fn create(&self, name: &InstanceName) -> wt_api::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Create(CreateInstance {
                name: name.clone(),
                source: self.git.url(),
                git_branch: None,
                git_ref: None,
                git_user_name: "WT E2E".to_owned(),
                git_user_email: "wt@example.invalid".to_owned(),
                vcpus: 1,
                memory_mib: 1024,
                disk_gib: 32,
                ssh_authorized_keys: vec![self.guest_public_key.clone()],
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

    pub(crate) fn finish_setup(&self, name: &InstanceName, agent: &SshAgent) -> wt_api::Instance {
        self.sync_inventory();
        let mut setup = start_world_setup(self.temp.path(), name, agent);
        let instance = wait_for_running(self.temp.path(), &self.server_config_path, name);
        let _ = setup.kill();
        let _ = setup.wait();
        instance
    }

    pub(crate) fn fork(&self, source: &InstanceName, name: &InstanceName) -> wt_api::Instance {
        let Response::Instance { instance } = call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Fork(ForkInstance {
                source: source.clone(),
                name: name.clone(),
            }),
        ) else {
            panic!("expected fork instance response");
        };
        *instance
    }

    pub(crate) fn delete(&self, name: &InstanceName) {
        call_api(
            self.temp.path(),
            &self.server_config_path,
            Operation::Delete { name: name.clone() },
        );
    }
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

pub(crate) fn guest_output(
    harness: &KvmHarness,
    name: &InstanceName,
    command: &str,
    action: &str,
) -> String {
    git_output(
        cmd!(
            "ssh",
            "-F",
            harness.temp.path().join(".ssh/config"),
            "-i",
            &harness.git.guest_key,
            format!("local.{name}-host"),
            command,
        ),
        action,
    )
}

pub(crate) fn wait_for_running(
    home: &Path,
    config: &Path,
    name: &InstanceName,
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
        assert!(Instant::now() < deadline, "timed out waiting for setup");
        std::thread::sleep(Duration::from_secs(2));
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

pub(crate) fn start_world_setup(home: &Path, name: &InstanceName, agent: &SshAgent) -> Child {
    cmd!(
        "ssh",
        "-F",
        home.join(".ssh/config"),
        format!("local.{name}")
    )
    .env("SSH_AUTH_SOCK", &agent.socket)
    .env("TERM", "xterm-ghostty")
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()
    .expect("start first-SSH world setup")
}

pub(crate) fn create_failed_setup_pane(home: &Path, name: &InstanceName) {
    let host = format!("local.{name}-host");
    for (arguments, action) in [
        (
            vec![
                "/usr/bin/tmux",
                "-f",
                "/usr/local/share/wt-tmux.conf",
                "new-session",
                "-d",
                "-s",
                "wt-app",
                "/bin/sleep",
                "600",
            ],
            "create setup session",
        ),
        (
            vec!["/usr/bin/tmux", "set-option", "-g", "remain-on-exit", "on"],
            "retain failed setup pane",
        ),
        (
            vec![
                "/usr/bin/tmux",
                "respawn-pane",
                "-k",
                "-t",
                "wt-app:0.0",
                "/bin/false",
            ],
            "fail setup pane",
        ),
    ] {
        let output = Command::new("ssh")
            .arg("-F")
            .arg(home.join(".ssh/config"))
            .arg(&host)
            .args(arguments)
            .output()
            .unwrap();
        ensure_success(action, &output).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let output = cmd!(
            "ssh",
            "-F",
            home.join(".ssh/config"),
            format!("local.{name}-host"),
            "/usr/bin/tmux",
            "display-message",
            "-p",
            "-t",
            "wt-app:0.0",
            "'#{pane_dead}'",
        )
        .output()
        .unwrap();
        if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "setup pane did not enter the failed state"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
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

pub(crate) fn assert_registry_cache_hit(since: u64) {
    let output = cmd!(
        "docker",
        "logs",
        "--since",
        since.to_string(),
        "wt-registry-cache",
    )
    .output()
    .expect("read registry cache logs");
    assert!(
        output.status.success(),
        "read registry cache logs: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let has_hit = String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|value| value["upstream_cache_status"].as_str() == Some("HIT"));
    assert!(
        has_hit,
        "registry cache recorded no HIT during world creation"
    );
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
