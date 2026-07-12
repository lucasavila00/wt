//! Production orchestration for one libvirt/KVM world.

mod devcontainer;
mod git;
mod guest_agent;
mod world;

use crate::{LibvirtConfig, ProvisionSpec, WorkerError, World, WorldWorker};
use ssh_key::{HashAlg, PublicKey};
use std::collections::BTreeSet;
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use virt::connect::Connect;
use virt::domain::Domain;
use virt::error::ErrorNumber;
use virt::network::Network;
use wt_command::cmd;

pub struct LibvirtWorker {
    config: LibvirtConfig,
    app_shell: Vec<u8>,
    app_pane: Vec<u8>,
    git_credentials: git::Credentials,
    registry_cache_url: String,
    registry_cache_ca: Vec<u8>,
}

impl LibvirtWorker {
    pub fn new(config: LibvirtConfig) -> Result<Self, WorkerError> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|error| worker_error("KVM is required but /dev/kvm is unavailable", error))?;
        require_file(&config.image, "guest image")?;
        require_file(&config.app_shell_binary, "guest app-shell binary")?;
        require_file(&config.app_pane_binary, "guest app-pane binary")?;
        let app_shell = fs::read(&config.app_shell_binary)
            .map_err(|error| worker_error("read guest app-shell binary", error))?;
        let app_pane = fs::read(&config.app_pane_binary)
            .map_err(|error| worker_error("read guest app-pane binary", error))?;
        if !config.worlds_dir.is_dir() {
            return Err(WorkerError::new(format!(
                "worlds directory not found: {}",
                config.worlds_dir.display()
            )));
        }
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let bridge = network_address(&connection, &config.network)?;
        let registry_cache_url = format!("http://{bridge}:{}", config.registry_cache_port);
        verify_registry_cache(&registry_cache_url)?;
        let registry_cache_ca = fs::read(config.registry_cache_state_dir.join("ca/ca.crt"))
            .map_err(|error| worker_error("read registry cache CA", error))?;
        let git_credentials =
            git::load_credentials(&config.git_identity_file, &config.git_known_hosts_file)?;
        Ok(Self {
            config,
            app_shell,
            app_pane,
            git_credentials,
            registry_cache_url,
            registry_cache_ca,
        })
    }

    fn provision_inner(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        wt_api::validate_ssh_git_source(spec.source)
            .map_err(|error| WorkerError::new(format!("Git source: {error}")))?;
        let private_git = &self.git_credentials;
        eprintln!("Creating KVM guest {}...", spec.name);
        let paths = world::Paths::new(&self.config.worlds_dir, spec.backend_id);
        fs::create_dir(&paths.directory)
            .map_err(|error| worker_error("create world directory", error))?;

        run(
            cmd!(
                "qemu-img",
                "create",
                "-q",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                &self.config.image,
                &paths.disk,
                format!("{}G", self.config.disk_gib),
            ),
            "create qcow2 overlay",
        )?;

        fs::write(
            &paths.user_data,
            world::cloud_config(
                &self.config.ssh_authorized_keys,
                &self.registry_cache_url,
                &self.registry_cache_ca,
            ),
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
        fs::write(&paths.network_config, world::network_config())
            .map_err(|error| worker_error("write cloud-init network-config", error))?;
        run(
            cmd!(
                "cloud-localds",
                "--network-config",
                &paths.network_config,
                &paths.seed,
                &paths.user_data,
                &paths.meta_data
            ),
            "create cloud-init seed",
        )?;

        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let xml = world::domain_xml(spec.backend_id, &paths, &self.config);
        let domain = Domain::define_xml(&connection, &xml)
            .map_err(|error| worker_error("define KVM domain", error))?;
        domain
            .create()
            .map_err(|error| worker_error("start KVM domain", error))?;

        // QEMU guest-agent is the provisioning channel. SSH is exposed to the
        // user, but wt does not depend on it to configure the world.
        eprintln!("Waiting for the guest agent...");
        let phase_started = Instant::now();
        self.wait_for_guest_agent(&domain)?;
        report_phase("guest agent and Docker readiness", phase_started);
        eprintln!("Waiting for guest networking...");
        let phase_started = Instant::now();
        let guest_ip = self.wait_for_ip(spec.backend_id)?;
        report_phase("guest networking", phase_started);
        let recipe_deadline = Instant::now() + self.config.recipe_timeout;
        eprintln!("Waiting for guest SSH...");
        let phase_started = Instant::now();
        self.wait_for_ssh(&guest_ip, recipe_deadline)?;
        let host_keys = self.read_host_keys(&domain, recipe_deadline)?;
        self.verify_ssh_endpoint(spec.backend_id, &guest_ip, &host_keys)?;
        report_phase("guest SSH readiness", phase_started);
        eprintln!("Cloning {}...", spec.source);
        let phase_started = Instant::now();
        git::clone_and_checkout(
            &domain,
            spec.source,
            private_git,
            spec.git_passphrase,
            recipe_deadline,
        )?;
        report_phase("Git clone and checkout", phase_started);
        eprintln!("Starting the repository devcontainer...");
        let phase_started = Instant::now();
        guest_agent::run_phase(
            &domain,
            "workspace ownership",
            "/bin/chown",
            &["-R", "wt:wt", "/workspace"],
            recipe_deadline,
        )?;
        guest_agent::run_phase(
            &domain,
            "devcontainer up",
            "/usr/sbin/runuser",
            &[
                "-u",
                "wt",
                "--",
                "/usr/bin/env",
                "HOME=/home/wt",
                "/usr/local/bin/devcontainer",
                "up",
                "--log-level",
                "debug",
                "--log-format",
                "text",
                "--workspace-folder",
                "/workspace",
            ],
            recipe_deadline,
        )?;
        report_phase("devcontainer up", phase_started);
        let phase_started = Instant::now();
        devcontainer::install_app_tools(&domain, &self.app_shell, &self.app_pane, recipe_deadline)?;
        guest_agent::run_phase(
            &domain,
            "devcontainer Git credentials",
            "/usr/local/bin/devcontainer",
            &[
                "exec",
                "--workspace-folder",
                "/workspace",
                "/bin/sh",
                "-c",
                "workspace=$(pwd -P) && git config --global --add safe.directory \"$workspace\" && directory=$(git rev-parse --git-common-dir)/wt && test -r \"$directory/identity\" && test -x \"$directory/ssh\" && test -r \"$directory/known_hosts\" && test -n \"$(git config --get core.sshCommand)\"",
            ],
            recipe_deadline,
        )?;
        report_phase("app shell and Git credential verification", phase_started);
        eprintln!("World {} is ready.", spec.name);
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
        let output = guest_agent::capture_phase(
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

    fn verify_ssh_endpoint(
        &self,
        backend_id: &str,
        guest_ip: &str,
        expected: &[String],
    ) -> Result<(), WorkerError> {
        let output = cmd!("/usr/bin/ssh-keyscan", "-T", "5", "-p", "22", guest_ip,)
            .output()
            .map_err(|error| worker_error("scan guest SSH host keys", error))?;
        let presented = String::from_utf8_lossy(&output.stdout);
        if host_keys_match(expected, &presented) {
            return Ok(());
        }
        let conflicts = self.domains_with_ip(backend_id, guest_ip)?;
        Err(endpoint_identity_error(
            guest_ip,
            expected,
            &presented,
            &conflicts,
            &self.config.worlds_dir,
        ))
    }

    fn domains_with_ip(
        &self,
        backend_id: &str,
        guest_ip: &str,
    ) -> Result<Vec<String>, WorkerError> {
        let connection = Connect::open(Some(crate::LIBVIRT_URI))
            .map_err(|error| worker_error("connect to libvirt", error))?;
        let domains = connection
            .list_all_domains(virt::sys::VIR_CONNECT_LIST_DOMAINS_ACTIVE)
            .map_err(|error| worker_error("list active libvirt domains", error))?;
        let mut conflicts = Vec::new();
        for domain in domains {
            let Ok(name) = domain.get_name() else {
                continue;
            };
            if name == backend_id || !name.starts_with("wt-") {
                continue;
            }
            let Ok(interfaces) =
                domain.interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_AGENT, 0)
            else {
                continue;
            };
            if interfaces
                .into_iter()
                .flat_map(|interface| interface.addrs)
                .any(|address| address.addr == guest_ip)
            {
                conflicts.push(name);
            }
        }
        conflicts.sort();
        Ok(conflicts)
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
        guest_agent::run_phase(
            domain,
            "cloud-init readiness",
            "/usr/bin/cloud-init",
            &["status", "--wait"],
            deadline,
        )?;
        guest_agent::run_phase(
            domain,
            "Docker and Compose readiness",
            "/bin/sh",
            &[
                "-lc",
                "test -f /var/lib/wt-registry-cache-ready && docker info >/dev/null && docker compose version >/dev/null",
            ],
            deadline,
        )?;
        Ok(())
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

fn network_address(connection: &Connect, name: &str) -> Result<String, WorkerError> {
    let network = Network::lookup_by_name(connection, name)
        .map_err(|error| worker_error("look up libvirt network", error))?;
    let xml = network
        .get_xml_desc(0)
        .map_err(|error| worker_error("read libvirt network XML", error))?;
    for quote in ['\'', '"'] {
        let needle = format!("address={quote}");
        for rest in xml.split(&needle).skip(1) {
            if let Some(address) = rest.split(quote).next() {
                if address.parse::<std::net::Ipv4Addr>().is_ok() {
                    return Ok(address.to_owned());
                }
            }
        }
    }
    Err(WorkerError::new(
        "configured libvirt network has no IPv4 bridge address",
    ))
}

fn verify_registry_cache(url: &str) -> Result<(), WorkerError> {
    run(
        cmd!(
            "curl",
            "-fsS",
            "--output",
            "/dev/null",
            format!("{url}/ca.crt")
        ),
        "verify registry cache",
    )
}

impl WorldWorker for LibvirtWorker {
    fn validate_git_passphrase(
        &self,
        passphrase: &wt_api::GitPassphrase,
    ) -> Result<(), WorkerError> {
        self.git_credentials.validate_passphrase(passphrase)
    }

    fn provision(&self, spec: &ProvisionSpec<'_>) -> Result<World, WorkerError> {
        match self.provision_inner(spec) {
            Ok(world) => Ok(world),
            Err(error) => {
                // A failed create must not leave a domain or overlay behind.
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
        // DHCP addresses can move; the per-world host keys are the stable SSH identity.
        let host_keys = self.read_host_keys(&domain, Instant::now() + self.config.boot_timeout)?;
        self.verify_ssh_endpoint(backend_id, &guest_ip, &host_keys)?;
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

fn normalized_host_keys(lines: &str) -> BTreeSet<String> {
    lines
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let first = fields.next()?;
            let (kind, data) = if is_host_key_kind(first) {
                (first, fields.next()?)
            } else {
                (fields.next()?, fields.next()?)
            };
            is_host_key_kind(kind).then(|| format!("{kind} {data}"))
        })
        .collect()
}

fn is_host_key_kind(value: &str) -> bool {
    value.starts_with("ssh-") || value.starts_with("ecdsa-") || value.starts_with("sk-")
}

fn host_keys_match(expected: &[String], presented: &str) -> bool {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    !expected.is_disjoint(&presented)
}

fn fingerprints(keys: &BTreeSet<String>) -> String {
    if keys.is_empty() {
        return "none".to_owned();
    }
    keys.iter()
        .map(|key| {
            PublicKey::from_openssh(key)
                .map(|key| key.fingerprint(HashAlg::Sha256).to_string())
                .unwrap_or_else(|_| "invalid-key".to_owned())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn endpoint_identity_error(
    guest_ip: &str,
    expected: &[String],
    presented: &str,
    conflicts: &[String],
    worlds_dir: &Path,
) -> WorkerError {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    let mut message = format!(
        "SSH endpoint identity mismatch at {guest_ip}:22: expected [{}], presented [{}]. WT refused to publish SSH access because another guest may be using this IP.",
        fingerprints(&expected),
        fingerprints(&presented),
    );
    if !conflicts.is_empty() {
        message.push_str(&format!(
            " Active WT domain(s) reporting {guest_ip}: {}.",
            conflicts.join(", ")
        ));
        let domain = &conflicts[0];
        message.push_str(&format!(
            " If `{domain}` is confirmed stale, run on the server: `virsh -c {} destroy {domain}`; `virsh -c {} undefine {domain} --nvram`; `rm -rf -- {}`. Then run `wt sync`. If it is managed, use `wt rm` instead.",
            crate::LIBVIRT_URI,
            crate::LIBVIRT_URI,
            worlds_dir.join(domain).display(),
        ));
    } else {
        message.push_str(
            " Inspect the server's DHCP and libvirt domain state, remove the stale guest, then run `wt sync`.",
        );
    }
    WorkerError::new(message)
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

fn run(mut command: Command, action: &str) -> Result<(), WorkerError> {
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

fn report_phase(label: &str, started: Instant) {
    eprintln!("{label} ready in {:.1}s.", started.elapsed().as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHIcU8rr2qppQ5sRKTKPoEp4dPLr+d1F7Eqog+U8AJbK";

    #[test]
    fn endpoint_keys_match_keyscan_output_without_comments() {
        let expected = vec![format!("{KEY} guest-comment")];
        let presented = format!("# banner\n192.0.2.2 {KEY}\n");

        assert!(host_keys_match(&expected, &presented));
        assert_eq!(
            normalized_host_keys(&presented),
            BTreeSet::from([KEY.to_owned()])
        );
    }

    #[test]
    fn mismatch_names_conflicting_domain_and_safe_recovery() {
        let error = endpoint_identity_error(
            "192.0.2.2",
            &[KEY.to_owned()],
            "",
            &["wt-deadbeef".to_owned()],
            Path::new("/var/lib/libvirt/images/wt"),
        )
        .to_string();

        assert!(error.contains("SSH endpoint identity mismatch at 192.0.2.2:22"));
        assert!(error.contains("Active WT domain(s) reporting 192.0.2.2: wt-deadbeef"));
        assert!(error.contains("undefine wt-deadbeef --nvram"));
        assert!(error.contains("/var/lib/libvirt/images/wt/wt-deadbeef"));
        assert!(error.contains("use `wt rm` instead"));
    }

    #[test]
    fn mismatch_without_known_conflict_requests_network_inspection() {
        let error = endpoint_identity_error(
            "192.0.2.2",
            &[KEY.to_owned()],
            "",
            &[],
            Path::new("/worlds"),
        )
        .to_string();

        assert!(error.contains("Inspect the server's DHCP and libvirt domain state"));
    }
}
