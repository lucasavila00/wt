//! Files and libvirt XML that define one KVM world.

use crate::LibvirtConfig;
use std::path::{Path, PathBuf};

pub(super) struct Paths {
    pub(super) directory: PathBuf,
    pub(super) disk: PathBuf,
    pub(super) seed: PathBuf,
    pub(super) user_data: PathBuf,
    pub(super) meta_data: PathBuf,
}

impl Paths {
    pub(super) fn new(root: &Path, backend_id: &str) -> Self {
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

pub(super) fn cloud_config(keys: &[String]) -> String {
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

pub(super) fn domain_xml(name: &str, paths: &Paths, config: &LibvirtConfig) -> String {
    let disk_path = paths.disk.to_string_lossy();
    let seed_path = paths.seed.to_string_lossy();
    let name = quick_xml::escape::escape(name);
    let disk = quick_xml::escape::escape(disk_path.as_ref());
    let seed = quick_xml::escape::escape(seed_path.as_ref());
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
