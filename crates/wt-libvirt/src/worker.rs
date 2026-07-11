use crate::{LibvirtConfig, ProvisionSpec, WorkerError, World, WorldWorker};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;

const GIT_BUNDLE_DIR: &str = "/workspace/.git/wt";
const GIT_SSH_COMMAND: &str =
    "sh -c 'exec \"$(git rev-parse --git-common-dir)/wt/ssh\" \"$@\"' wt-ssh";
const GIT_SSH_WRAPPER: &[u8] = br#"#!/bin/sh
set -eu
directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
runtime=$(mktemp -d "${TMPDIR:-/tmp}/wt-git.XXXXXX")
trap 'rm -rf "$runtime"' EXIT HUP INT TERM
install -m 0600 "$directory/identity" "$runtime/identity"
/usr/bin/ssh \
  -i "$runtime/identity" \
  -o IdentitiesOnly=yes \
  -o UserKnownHostsFile="$directory/known_hosts" \
  -o StrictHostKeyChecking=yes \
  "$@"
"#;

pub struct LibvirtWorker {
    config: LibvirtConfig,
}

impl LibvirtWorker {
    pub fn new(config: LibvirtConfig) -> Result<Self, WorkerError> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|error| worker_error("KVM is required but /dev/kvm is unavailable", error))?;
        require_file(&config.image, "guest image")?;
        if !config.worlds_dir.is_dir() {
            return Err(WorkerError::new(format!(
                "worlds directory not found: {}",
                config.worlds_dir.display()
            )));
        }
        Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        Ok(Self { config })
    }

    fn provision_inner(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        wt_api::validate_ssh_git_source(spec.source)
            .map_err(|error| WorkerError::new(format!("Git source: {error}")))?;
        let private_git = self.load_private_git(spec.identity_file)?;
        let paths = WorldPaths::new(&self.config.worlds_dir, spec.backend_id);
        fs::create_dir(&paths.directory)
            .map_err(|error| worker_error("create world directory", error))?;

        run(
            Command::new("qemu-img")
                .args(["create", "-q", "-f", "qcow2", "-F", "qcow2", "-b"])
                .arg(&self.config.image)
                .arg(&paths.disk)
                .arg(format!("{}G", self.config.disk_gib)),
            "create qcow2 overlay",
        )?;

        fs::write(
            &paths.user_data,
            cloud_config(&self.config.ssh_authorized_keys),
        )
        .map_err(|error| worker_error("write cloud-init user-data", error))?;
        fs::write(
            &paths.meta_data,
            format!(
                "instance-id: {}\nlocal-hostname: {}\n",
                spec.backend_id, spec.name
            ),
        )
        .map_err(|error| worker_error("write cloud-init meta-data", error))?;
        run(
            Command::new("cloud-localds")
                .arg(&paths.seed)
                .arg(&paths.user_data)
                .arg(&paths.meta_data),
            "create cloud-init seed",
        )?;

        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let xml = domain_xml(spec.backend_id, &paths.disk, &paths.seed, &self.config);
        let domain = Domain::define_xml(&connection, &xml)
            .map_err(|error| worker_error("define KVM domain", error))?;
        domain
            .create()
            .map_err(|error| worker_error("start KVM domain", error))?;

        self.wait_for_guest_agent(&domain)?;
        let guest_ip = self.wait_for_ip(spec.backend_id)?;
        let recipe_deadline = Instant::now() + self.config.recipe_timeout;
        self.wait_for_ssh(&guest_ip, recipe_deadline)?;
        let host_keys = self.read_host_keys(&domain, recipe_deadline)?;
        self.clone_private(
            &domain,
            spec.source,
            spec.git_ref,
            &private_git,
            recipe_deadline,
        )?;
        self.run_phase(
            &domain,
            "workspace ownership",
            "/bin/chown",
            &["-R", "wt:wt", "/workspace"],
            recipe_deadline,
        )?;
        self.run_phase(
            &domain,
            "devcontainer up",
            "/usr/local/bin/devcontainer",
            &["up", "--workspace-folder", "/workspace"],
            recipe_deadline,
        )?;
        self.run_phase(
            &domain,
            "devcontainer Git credentials",
            "/usr/local/bin/devcontainer",
            &[
                "exec",
                "--workspace-folder",
                "/workspace",
                "/bin/sh",
                "-c",
                "directory=$(git rev-parse --git-common-dir)/wt && test -r \"$directory/identity\" && test -x \"$directory/ssh\" && test -r \"$directory/known_hosts\" && test -n \"$(git config --get core.sshCommand)\"",
            ],
            recipe_deadline,
        )?;
        Ok(World {
            guest_ip: guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: guest_ip,
                port: 22,
                host_keys,
            },
        })
    }

    fn run_phase(
        &self,
        domain: &Domain,
        phase: &str,
        path: &str,
        args: &[&str],
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkerError> {
        let output = guest_exec(domain, path, args, deadline)
            .map_err(|error| WorkerError::new(format!("{phase}: {error}")))?;
        if output.exit_code != 0 {
            return Err(WorkerError::new(format!(
                "{phase}: exit code {}: {}",
                output.exit_code,
                tail_output(&output.stdout, &output.stderr)
            )));
        }
        Ok(output.stdout)
    }

    fn wait_for_ssh(&self, guest_ip: &str, deadline: Instant) -> Result<(), WorkerError> {
        let address: SocketAddr = format!("{guest_ip}:22")
            .parse()
            .map_err(|error| worker_error("parse guest SSH address", error))?;
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(
                    "SSH readiness: timed out waiting for port 22",
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn read_host_keys(
        &self,
        domain: &Domain,
        deadline: Instant,
    ) -> Result<Vec<String>, WorkerError> {
        let output = self.run_phase(
            domain,
            "SSH host keys",
            "/bin/sh",
            &["-c", "cat /etc/ssh/ssh_host_*_key.pub"],
            deadline,
        )?;
        let keys = String::from_utf8_lossy(&output)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return Err(WorkerError::new(
                "SSH host keys: guest returned no public host keys",
            ));
        }
        Ok(keys)
    }

    fn load_private_git(&self, identity_file: &str) -> Result<PrivateGit, WorkerError> {
        let path = identity_file;
        let identity = fs::read(path)
            .map_err(|error| worker_error("Git identity: read private key", error))?;
        let unencrypted = private_key_accepts_passphrase(path, "")?;
        let passphrase = if unencrypted {
            None
        } else {
            let value = rpassword::prompt_password(format!("Passphrase for {path}: "))
                .map_err(|error| worker_error("Git identity: read passphrase", error))?;
            if !private_key_accepts_passphrase(path, &value)? {
                return Err(WorkerError::new(
                    "Git identity: invalid private key or passphrase",
                ));
            }
            Some(value.into_bytes())
        };
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| WorkerError::new("Git host trust: HOME is not set"))?;
        let known_hosts = fs::read(home.join(".ssh/known_hosts"))
            .map_err(|error| worker_error("Git host trust: read ~/.ssh/known_hosts", error))?;
        Ok(PrivateGit {
            identity,
            passphrase,
            known_hosts,
        })
    }

    fn clone_private(
        &self,
        domain: &Domain,
        source: &str,
        git_ref: Option<&str>,
        credentials: &PrivateGit,
        deadline: Instant,
    ) -> Result<(), WorkerError> {
        self.run_phase(
            domain,
            "Git credentials",
            "/usr/bin/install",
            &["-d", "-m", "0700", "/run/wt-git"],
            deadline,
        )?;
        let result = (|| {
            guest_write(domain, "/run/wt-git/identity", &credentials.identity)?;
            guest_write(domain, "/run/wt-git/known_hosts", &credentials.known_hosts)?;
            if let Some(passphrase) = &credentials.passphrase {
                guest_write(domain, "/run/wt-git/passphrase", passphrase)?;
                guest_write(
                    domain,
                    "/run/wt-git/askpass",
                    b"#!/bin/sh\ncat /run/wt-git/passphrase\n",
                )?;
                self.run_phase(
                    domain,
                    "Git credentials",
                    "/bin/chmod",
                    &["0700", "/run/wt-git/askpass"],
                    deadline,
                )?;
            }
            self.run_phase(
                domain,
                "Git credentials",
                "/bin/chmod",
                &["0600", "/run/wt-git/identity", "/run/wt-git/known_hosts"],
                deadline,
            )?;
            let ssh_command = "ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes";
            let mut environment = vec![format!("GIT_SSH_COMMAND={ssh_command}")];
            if credentials.passphrase.is_some() {
                environment.extend([
                    "SSH_ASKPASS=/run/wt-git/askpass".to_owned(),
                    "SSH_ASKPASS_REQUIRE=force".to_owned(),
                    "DISPLAY=wt:0".to_owned(),
                ]);
            }
            let mut args = environment.iter().map(String::as_str).collect::<Vec<_>>();
            args.extend(["/usr/bin/git", "clone", "--", source, "/workspace"]);
            self.run_phase(domain, "Git clone", "/usr/bin/env", &args, deadline)?;
            if let Some(git_ref) = git_ref {
                let mut args = environment.iter().map(String::as_str).collect::<Vec<_>>();
                args.extend([
                    "/usr/bin/git",
                    "-C",
                    "/workspace",
                    "fetch",
                    "origin",
                    git_ref,
                ]);
                self.run_phase(domain, "Git fetch ref", "/usr/bin/env", &args, deadline)?;
                let mut args = environment.iter().map(String::as_str).collect::<Vec<_>>();
                args.extend([
                    "/usr/bin/git",
                    "-C",
                    "/workspace",
                    "checkout",
                    "--detach",
                    "FETCH_HEAD",
                ]);
                self.run_phase(domain, "Git checkout ref", "/usr/bin/env", &args, deadline)?;
            }
            self.install_git_bundle(domain, credentials, deadline)
        })();
        let _ = guest_exec(domain, "/bin/rm", &["-rf", "/run/wt-git"], deadline);
        result
    }

    fn install_git_bundle(
        &self,
        domain: &Domain,
        credentials: &PrivateGit,
        deadline: Instant,
    ) -> Result<(), WorkerError> {
        self.run_phase(
            domain,
            "Git credentials",
            "/usr/bin/install",
            &["-d", "-m", "0755", GIT_BUNDLE_DIR],
            deadline,
        )?;
        guest_write(
            domain,
            &format!("{GIT_BUNDLE_DIR}/identity"),
            &credentials.identity,
        )?;
        guest_write(
            domain,
            &format!("{GIT_BUNDLE_DIR}/known_hosts"),
            &credentials.known_hosts,
        )?;
        guest_write(domain, &format!("{GIT_BUNDLE_DIR}/ssh"), GIT_SSH_WRAPPER)?;
        self.run_phase(
            domain,
            "Git credentials",
            "/bin/chmod",
            &[
                "0444",
                &format!("{GIT_BUNDLE_DIR}/identity"),
                &format!("{GIT_BUNDLE_DIR}/known_hosts"),
            ],
            deadline,
        )?;
        self.run_phase(
            domain,
            "Git credentials",
            "/bin/chmod",
            &["0555", &format!("{GIT_BUNDLE_DIR}/ssh")],
            deadline,
        )?;
        self.run_phase(
            domain,
            "Git credentials",
            "/usr/bin/git",
            &[
                "-C",
                "/workspace",
                "config",
                "--local",
                "core.sshCommand",
                GIT_SSH_COMMAND,
            ],
            deadline,
        )?;
        Ok(())
    }

    fn wait_for_ip(&self, backend_id: &str) -> Result<String, WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            if let Some(host) = self.domain_ip(backend_id)? {
                return Ok(host);
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for IP for domain {backend_id}"
                )));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn wait_for_guest_agent(&self, domain: &Domain) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.config.boot_timeout;
        loop {
            if domain
                .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                .is_ok()
            {
                return self.verify_guest_tools(domain, deadline);
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new("timed out waiting for QEMU guest agent"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn verify_guest_tools(&self, domain: &Domain, deadline: Instant) -> Result<(), WorkerError> {
        let request = serde_json::json!({
            "execute": "guest-exec",
            "arguments": {
                "path": "/bin/sh",
                "arg": [
                    "-lc",
                    "docker info >/dev/null && docker compose version >/dev/null"
                ],
                "capture-output": true
            }
        });
        let response = domain
            .qemu_agent_command(&request.to_string(), 10, 0)
            .map_err(|error| worker_error("start guest readiness command", error))?;
        let response: serde_json::Value = serde_json::from_str(&response)
            .map_err(|error| worker_error("decode guest agent response", error))?;
        let pid = response["return"]["pid"]
            .as_u64()
            .ok_or_else(|| WorkerError::new("guest agent did not return an execution pid"))?;

        loop {
            let request = serde_json::json!({
                "execute": "guest-exec-status",
                "arguments": { "pid": pid }
            });
            let response = domain
                .qemu_agent_command(&request.to_string(), 10, 0)
                .map_err(|error| worker_error("read guest readiness command", error))?;
            let response: serde_json::Value = serde_json::from_str(&response)
                .map_err(|error| worker_error("decode guest agent response", error))?;
            let result = &response["return"];
            if result["exited"].as_bool() == Some(true) {
                return match result["exitcode"].as_i64() {
                    Some(0) => Ok(()),
                    exit_code => Err(WorkerError::new(format!(
                        "Docker or Compose readiness check failed with exit code {exit_code:?}"
                    ))),
                };
            }
            if Instant::now() >= deadline {
                return Err(WorkerError::new(
                    "timed out waiting for Docker and Compose readiness",
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn domain_ip(&self, backend_id: &str) -> Result<Option<String>, WorkerError> {
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = match Domain::lookup_by_name(&connection, backend_id) {
            Ok(domain) => domain,
            Err(error) if error.code() == ErrorNumber::NoDomain => return Ok(None),
            Err(error) => return Err(worker_error("look up libvirt domain", error)),
        };
        let interfaces = domain
            .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
            .map_err(|error| worker_error("get domain interface addresses", error))?;
        Ok(interfaces
            .into_iter()
            .flat_map(|interface| interface.addrs)
            .find_map(|address| {
                let ip = address.addr.parse::<IpAddr>().ok()?;
                (ip.is_ipv4() && !ip.is_loopback()).then(|| ip.to_string())
            }))
    }

    fn remove_domain(&self, backend_id: &str) -> Result<(), WorkerError> {
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = match Domain::lookup_by_name(&connection, backend_id) {
            Ok(domain) => domain,
            Err(error) if error.code() == ErrorNumber::NoDomain => return Ok(()),
            Err(error) => return Err(worker_error("look up libvirt domain", error)),
        };
        if domain
            .is_active()
            .map_err(|error| worker_error("check domain state", error))?
        {
            domain
                .destroy()
                .map_err(|error| worker_error("destroy domain", error))?;
        }
        domain
            .undefine_flags(virt::sys::VIR_DOMAIN_UNDEFINE_NVRAM)
            .map_err(|error| worker_error("undefine domain", error))?;
        Ok(())
    }

    fn remove_files(&self, backend_id: &str) -> Result<(), WorkerError> {
        let directory = self.config.worlds_dir.join(backend_id);
        if directory.exists() {
            fs::remove_dir_all(&directory)
                .map_err(|error| worker_error("remove world files", error))?;
        }
        Ok(())
    }
}

impl WorldWorker for LibvirtWorker {
    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        match self.provision_inner(spec) {
            Ok(world) => Ok(world),
            Err(error) => {
                let _ = self.remove_domain(spec.backend_id);
                let _ = self.remove_files(spec.backend_id);
                Err(error)
            }
        }
    }

    fn destroy(&self, backend_id: &str) -> Result<(), WorkerError> {
        self.remove_domain(backend_id)?;
        self.remove_files(backend_id)
    }

    fn inspect(&self, backend_id: &str) -> Result<Option<World>, WorkerError> {
        let Some(guest_ip) = self.domain_ip(backend_id)? else {
            return Ok(None);
        };
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domain = Domain::lookup_by_name(&connection, backend_id)
            .map_err(|error| worker_error("look up libvirt domain", error))?;
        let host_keys = self.read_host_keys(&domain, Instant::now() + self.config.boot_timeout)?;
        Ok(Some(World {
            guest_ip: guest_ip.clone(),
            ssh: wt_api::SshAccess {
                user: "wt".to_owned(),
                host: guest_ip,
                port: 22,
                host_keys,
            },
        }))
    }
}

struct PrivateGit {
    identity: Vec<u8>,
    passphrase: Option<Vec<u8>>,
    known_hosts: Vec<u8>,
}

struct GuestOutput {
    exit_code: i64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn guest_exec(
    domain: &Domain,
    path: &str,
    args: &[&str],
    deadline: Instant,
) -> Result<GuestOutput, WorkerError> {
    if Instant::now() >= deadline {
        return Err(WorkerError::new("recipe deadline exceeded"));
    }
    let request = serde_json::json!({
        "execute": "guest-exec",
        "arguments": { "path": path, "arg": args, "capture-output": true }
    });
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| worker_error("start guest command", error))?;
    let response: serde_json::Value = serde_json::from_str(&response)
        .map_err(|error| worker_error("decode guest command response", error))?;
    let pid = response["return"]["pid"]
        .as_u64()
        .ok_or_else(|| WorkerError::new("guest agent did not return an execution pid"))?;
    loop {
        let request = serde_json::json!({
            "execute": "guest-exec-status",
            "arguments": { "pid": pid }
        });
        let response = domain
            .qemu_agent_command(&request.to_string(), 10, 0)
            .map_err(|error| worker_error("read guest command", error))?;
        let response: serde_json::Value = serde_json::from_str(&response)
            .map_err(|error| worker_error("decode guest command status", error))?;
        let result = &response["return"];
        if result["exited"].as_bool() == Some(true) {
            return Ok(GuestOutput {
                exit_code: result["exitcode"].as_i64().unwrap_or(-1),
                stdout: decode_guest_data(result.get("out-data"))?,
                stderr: decode_guest_data(result.get("err-data"))?,
            });
        }
        if Instant::now() >= deadline {
            return Err(WorkerError::new("recipe deadline exceeded"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn decode_guest_data(value: Option<&serde_json::Value>) -> Result<Vec<u8>, WorkerError> {
    let Some(value) = value.and_then(serde_json::Value::as_str) else {
        return Ok(Vec::new());
    };
    BASE64
        .decode(value)
        .map_err(|error| worker_error("decode guest command output", error))
}

fn guest_write(domain: &Domain, path: &str, contents: &[u8]) -> Result<(), WorkerError> {
    let request = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": { "path": path, "mode": "w" }
    });
    let response = domain
        .qemu_agent_command(&request.to_string(), 10, 0)
        .map_err(|error| worker_error("open guest credential file", error))?;
    let response: serde_json::Value = serde_json::from_str(&response)
        .map_err(|error| worker_error("decode guest file response", error))?;
    let handle = response["return"]
        .as_i64()
        .ok_or_else(|| WorkerError::new("guest agent did not return a file handle"))?;
    let result = (|| {
        for chunk in contents.chunks(48 * 1024) {
            let request = serde_json::json!({
                "execute": "guest-file-write",
                "arguments": { "handle": handle, "buf-b64": BASE64.encode(chunk) }
            });
            domain
                .qemu_agent_command(&request.to_string(), 10, 0)
                .map_err(|error| worker_error("write guest credential file", error))?;
        }
        Ok(())
    })();
    let close = serde_json::json!({
        "execute": "guest-file-close", "arguments": { "handle": handle }
    });
    let _ = domain.qemu_agent_command(&close.to_string(), 10, 0);
    result
}

fn tail_output(stdout: &[u8], stderr: &[u8]) -> String {
    const LIMIT: usize = 64 * 1024;
    let mut combined = Vec::with_capacity(stdout.len() + stderr.len() + 1);
    combined.extend_from_slice(stdout);
    if !stdout.is_empty() && !stderr.is_empty() {
        combined.push(b'\n');
    }
    combined.extend_from_slice(stderr);
    let start = combined.len().saturating_sub(LIMIT);
    String::from_utf8_lossy(&combined[start..])
        .trim()
        .to_owned()
}

fn cloud_config(keys: &[String]) -> String {
    let keys = keys
        .iter()
        .map(|key| {
            format!(
                "      - {}",
                serde_json::to_string(key).expect("serialize public key")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "#cloud-config\nssh_deletekeys: true\nssh_genkeytypes: [rsa, ecdsa, ed25519]\nusers:\n  - default\n  - name: wt\n    groups: [docker]\n    shell: /bin/bash\n    lock_passwd: true\n    ssh_authorized_keys:\n{keys}\nruncmd:\n  - [install, -d, -o, wt, -g, wt, /workspace]\n  - [systemctl, enable, --now, ssh.service]\n"
    )
}

struct WorldPaths {
    directory: PathBuf,
    disk: PathBuf,
    seed: PathBuf,
    user_data: PathBuf,
    meta_data: PathBuf,
}

impl WorldPaths {
    fn new(root: &Path, backend_id: &str) -> Self {
        let directory = root.join(backend_id);
        Self {
            disk: directory.join("disk.qcow2"),
            seed: directory.join("seed.img"),
            user_data: directory.join("user-data"),
            meta_data: directory.join("meta-data"),
            directory,
        }
    }
}

fn domain_xml(name: &str, disk: &Path, seed: &Path, config: &LibvirtConfig) -> String {
    let disk = disk.to_string_lossy();
    let seed = seed.to_string_lossy();
    let name = quick_xml::escape::escape(name);
    let disk = quick_xml::escape::escape(disk.as_ref());
    let seed = quick_xml::escape::escape(seed.as_ref());
    let network = quick_xml::escape::escape(&config.network);
    let architecture = quick_xml::escape::escape(crate::GUEST_ARCHITECTURE);
    let machine = quick_xml::escape::escape(crate::GUEST_MACHINE);
    let memory_mib = config.memory_mib;
    let vcpus = config.vcpus;
    format!(
        "<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory_mib}</memory>
  <vcpu>{vcpus}</vcpu>
  <os firmware='efi'>
    <type arch='{architecture}' machine='{machine}'>hvm</type>
    <firmware><feature enabled='no' name='secure-boot'/></firmware>
  </os>
  <features><acpi/><apic/></features>
  <cpu mode='host-passthrough' check='none'/>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='{seed}'/>
      <target dev='sda' bus='sata'/>
      <readonly/>
    </disk>
    <interface type='network'>
      <source network='{network}'/>
      <model type='virtio'/>
    </interface>
    <channel type='unix'>
      <target type='virtio' name='org.qemu.guest_agent.0'/>
    </channel>
    <serial type='pty'><target port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
    <rng model='virtio'><backend model='random'>/dev/urandom</backend></rng>
  </devices>
</domain>"
    )
}

fn require_file(path: &Path, label: &str) -> Result<(), WorkerError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(WorkerError::new(format!(
            "{label} not found: {}",
            path.display()
        )))
    }
}

fn run(command: &mut Command, action: &str) -> Result<(), WorkerError> {
    let output = command
        .output()
        .map_err(|error| worker_error(action, error))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(WorkerError::new(format!("{action}: {stderr}")))
}

fn worker_error(action: &str, error: impl std::fmt::Display) -> WorkerError {
    WorkerError::new(format!("{action}: {error}"))
}

fn private_key_accepts_passphrase(path: &str, passphrase: &str) -> Result<bool, WorkerError> {
    Ok(Command::new("ssh-keygen")
        .args(["-y", "-P", passphrase, "-f", path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| worker_error("Git identity: inspect private key", error))?
        .success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn persistent_ssh_command_resolves_from_nested_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        fs::create_dir(&repository).unwrap();
        run(
            Command::new("git").args(["init", "-q"]).arg(&repository),
            "initialize test repository",
        )
        .unwrap();
        let bundle = repository.join(".git/wt");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("identity"), "not-read-by-ssh-g\n").unwrap();
        fs::write(bundle.join("known_hosts"), "").unwrap();
        fs::write(bundle.join("ssh"), GIT_SSH_WRAPPER).unwrap();
        fs::set_permissions(bundle.join("ssh"), fs::Permissions::from_mode(0o555)).unwrap();
        run(
            Command::new("git").args(["-C"]).arg(&repository).args([
                "config",
                "core.sshCommand",
                GIT_SSH_COMMAND,
            ]),
            "configure test SSH command",
        )
        .unwrap();
        let nested = repository.join("nested");
        fs::create_dir(&nested).unwrap();
        let runtime = temp.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{GIT_SSH_COMMAND} -T -G example.test >/dev/null"))
            .current_dir(&nested)
            .env("TMPDIR", &runtime)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_dir(runtime).unwrap().count(), 0);
    }

    #[test]
    fn detects_encrypted_private_key_passphrases() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("identity");
        run(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "secret", "-f"])
                .arg(&key),
            "generate encrypted key",
        )
        .unwrap();
        let key = key.to_str().unwrap();
        assert!(!private_key_accepts_passphrase(key, "").unwrap());
        assert!(!private_key_accepts_passphrase(key, "wrong").unwrap());
        assert!(private_key_accepts_passphrase(key, "secret").unwrap());
    }
}
