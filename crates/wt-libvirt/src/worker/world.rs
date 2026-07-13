//! Files and libvirt XML that define one KVM world.

use crate::MachineConfig;
use std::path::{Path, PathBuf};

pub(super) struct Paths {
    pub(super) directory: PathBuf,
    pub(super) disk: PathBuf,
    pub(super) seed: PathBuf,
    pub(super) user_data: PathBuf,
    pub(super) meta_data: PathBuf,
    pub(super) network_config: PathBuf,
}

impl Paths {
    pub(super) fn new(root: &Path, provider_id: &wt_provider::ProviderId) -> Self {
        let directory = root.join(provider_id.as_str());
        Self {
            disk: directory.join("disk.qcow2"),
            seed: directory.join("seed.img"),
            user_data: directory.join("user-data"),
            meta_data: directory.join("meta-data"),
            network_config: directory.join("network-config"),
            directory,
        }
    }
}

pub(super) fn network_config() -> &'static str {
    "version: 2\nethernets:\n  primary:\n    match:\n      name: \"en*\"\n    dhcp4: true\n    dhcp-identifier: mac\n"
}

pub(super) fn cloud_config() -> &'static str {
    "#cloud-config\npackage_update: true\npackages:\n  - qemu-guest-agent\nruncmd:\n  - [systemctl, enable, --now, qemu-guest-agent.service]\n"
}

pub(super) fn domain_xml(
    provider_id: &wt_provider::ProviderId,
    paths: &Paths,
    config: &MachineConfig,
    spec: &wt_provider::MachineSpec,
) -> String {
    let disk_path = paths.disk.to_string_lossy();
    let seed_path = paths.seed.to_string_lossy();
    let name = quick_xml::escape::escape(provider_id.as_str());
    let disk = quick_xml::escape::escape(disk_path.as_ref());
    let seed = quick_xml::escape::escape(seed_path.as_ref());
    let network = quick_xml::escape::escape(&config.network);
    let architecture = quick_xml::escape::escape(crate::GUEST_ARCHITECTURE);
    let machine = quick_xml::escape::escape(crate::GUEST_MACHINE);
    let memory_mib = spec.memory_mib;
    let vcpus = spec.vcpus;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_dhcp_identity_uses_the_unique_interface_mac() {
        insta::assert_snapshot!(network_config(), @r###"
        version: 2
        ethernets:
          primary:
            match:
              name: "en*"
            dhcp4: true
            dhcp-identifier: mac
        "###);
    }
}
