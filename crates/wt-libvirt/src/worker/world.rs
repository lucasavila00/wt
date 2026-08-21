//! Files and libvirt XML that define one KVM world.

use crate::MachineConfig;
use std::path::{Path, PathBuf};

pub(super) struct Paths {
    pub(super) directory: PathBuf,
    pub(super) seed: PathBuf,
    pub(super) user_data: PathBuf,
    pub(super) vendor_data: PathBuf,
    pub(super) meta_data: PathBuf,
    pub(super) network_config: PathBuf,
}

impl Paths {
    pub(super) fn new(root: &Path, provider_id: &wt_provider::ProviderId) -> Self {
        let directory = root.join(provider_id.as_str());
        Self {
            seed: directory.join("seed.img"),
            user_data: directory.join("user-data"),
            vendor_data: directory.join("vendor-data"),
            meta_data: directory.join("meta-data"),
            network_config: directory.join("network-config"),
            directory,
        }
    }
}

pub(super) fn disk_path(root: &Path, disk_id: uuid::Uuid) -> PathBuf {
    root.join("disks").join(format!("{disk_id}.qcow2"))
}

pub(super) fn network_config() -> &'static str {
    "version: 2\nethernets:\n  primary:\n    match:\n      name: \"en*\"\n    dhcp4: true\n    dhcp-identifier: mac\n"
}

pub(super) fn domain_xml(
    provider_id: &wt_provider::ProviderId,
    paths: &Paths,
    disk_path: &Path,
    config: &MachineConfig,
    spec: &wt_provider::MachineSpec,
    network_enabled: bool,
) -> String {
    let disk_path = disk_path.to_string_lossy();
    let seed_path = paths.seed.to_string_lossy();
    let name = quick_xml::escape::escape(provider_id.as_str());
    let disk = quick_xml::escape::escape(disk_path.as_ref());
    let seed = quick_xml::escape::escape(seed_path.as_ref());
    let network = quick_xml::escape::escape(&config.network);
    let architecture = quick_xml::escape::escape(crate::GUEST_ARCHITECTURE);
    let machine = quick_xml::escape::escape(crate::GUEST_MACHINE);
    let memory_mib = spec.memory_mib;
    let vcpus = spec.vcpus;
    let mac = mac_address(provider_id);
    let memory_backing = if config.shared_folders.is_empty() {
        String::new()
    } else {
        "  <memoryBacking>\n    <source type='memfd'/>\n    <access mode='shared'/>\n  </memoryBacking>\n".to_owned()
    };
    let shared_folders = config
        .shared_folders
        .iter()
        .map(|folder| {
            let source = folder.source.to_string_lossy();
            let source = quick_xml::escape::escape(source.as_ref());
            let tag = quick_xml::escape::escape(&folder.tag);
            format!(
                "    <filesystem type='mount' accessmode='passthrough'>\n      <driver type='virtiofs'/>\n      <source dir='{source}'/>\n      <target dir='{tag}'/>\n    </filesystem>\n"
            )
        })
        .collect::<String>();
    let interface = if network_enabled {
        format!(
            "    <interface type='network'>\n      <mac address='{mac}'/>\n      <source network='{network}'/>\n      <model type='virtio'/>\n    </interface>\n"
        )
    } else {
        String::new()
    };
    format!(
        "<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory_mib}</memory>
{memory_backing}  <vcpu>{vcpus}</vcpu>
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
{interface}{shared_folders}    <channel type='unix'>
      <target type='virtio' name='org.qemu.guest_agent.0'/>
    </channel>
    <vsock model='virtio'><cid auto='yes'/></vsock>
    <serial type='pty'><target port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
    <rng model='virtio'><backend model='random'>/dev/urandom</backend></rng>
  </devices>
</domain>"
    )
}

fn mac_address(provider_id: &wt_provider::ProviderId) -> String {
    let suffix = &provider_id.as_str()[3..];
    format!(
        "52:54:00:{}:{}:{}",
        &suffix[0..2],
        &suffix[2..4],
        &suffix[4..6]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MachineConfig, SharedFolder};
    use std::time::Duration;

    fn test_domain_xml(shared_folders: Vec<SharedFolder>) -> String {
        let provider_id =
            wt_provider::ProviderId::parse("wt-0123456789abcdef0123456789abcdef").unwrap();
        let paths = Paths::new(Path::new("/var/lib/libvirt/images/wt"), &provider_id);
        let config = MachineConfig {
            image: PathBuf::from("/var/lib/wt/images/golden.qcow2"),
            worlds_dir: PathBuf::from("/var/lib/libvirt/images/wt"),
            network: "default & private".to_owned(),
            boot_timeout: Duration::from_secs(300),
            shared_folders,
        };
        let spec = wt_provider::MachineSpec {
            provider_id: provider_id.clone(),
            disk_id: uuid::Uuid::nil(),
            memory_mib: 4096,
            vcpus: 4,
            disk_gib: 32,
            cloud_init: wt_provider::NoCloudConfig::default(),
        };
        domain_xml(
            &provider_id,
            &paths,
            Path::new("/var/lib/libvirt/images/wt/disks/world & head.qcow2"),
            &config,
            &spec,
            true,
        )
    }

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

    #[test]
    fn domain_without_shared_folders_has_no_virtiofs_support() {
        insta::assert_snapshot!(
            "domain_xml_without_shared_folders",
            test_domain_xml(Vec::new())
        );
    }

    #[test]
    fn domain_with_one_shared_folder_has_virtiofs_support() {
        insta::assert_snapshot!(
            "domain_xml_with_one_shared_folder",
            test_domain_xml(vec![SharedFolder {
                source: PathBuf::from("/home/wt/.codex/sessions"),
                tag: "wt-shared-0".to_owned(),
            }])
        );
    }

    #[test]
    fn domain_with_two_shared_folders_escapes_sources_and_keeps_stable_tags() {
        insta::assert_snapshot!(
            "domain_xml_with_two_shared_folders",
            test_domain_xml(vec![
                SharedFolder {
                    source: PathBuf::from("/var/lib/wt/shared/codex & sessions"),
                    tag: "wt-shared-0".to_owned(),
                },
                SharedFolder {
                    source: PathBuf::from("/var/lib/wt/shared/notes"),
                    tag: "wt-shared-1".to_owned(),
                },
            ])
        );
    }
}
