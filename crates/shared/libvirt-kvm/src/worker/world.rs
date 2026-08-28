//! Files and libvirt XML that define one KVM world.

use crate::MachineConfig;
use std::path::{Path, PathBuf};

pub(super) fn disk_path(root: &Path, world_id: wt_world::WorldId) -> PathBuf {
    root.join("disks").join(format!("{world_id}.qcow2"))
}

pub(super) fn domain_xml(
    domain_name: &crate::DomainName,
    disk_path: &Path,
    config: &MachineConfig,
    spec: &crate::MachineSpec,
    network_enabled: bool,
) -> String {
    let disk_path = disk_path.to_string_lossy();
    let name = quick_xml::escape::escape(domain_name.as_str());
    let disk = quick_xml::escape::escape(disk_path.as_ref());
    let network = quick_xml::escape::escape(&config.network);
    let architecture = quick_xml::escape::escape(crate::GUEST_ARCHITECTURE);
    let machine = quick_xml::escape::escape(crate::GUEST_MACHINE);
    let memory_mib = spec.memory_mib;
    let vcpus = spec.vcpus;
    let mac = mac_address(domain_name);
    let memory_backing = if config.shared_mounts.is_none() {
        String::new()
    } else {
        "  <memoryBacking>\n    <source type='memfd'/>\n    <access mode='shared'/>\n  </memoryBacking>\n".to_owned()
    };
    let shared_mounts = config.shared_mounts.as_ref().map_or_else(String::new, |mounts| {
        let sessions_path = mounts
            .sessions_root
            .join(spec.world_id.to_string())
            .to_string_lossy()
            .into_owned();
        let auth_path = mounts.auth.to_string_lossy();
        let ssh_authorized_keys_path = mounts.ssh_authorized_keys.to_string_lossy();
        let sessions = quick_xml::escape::escape(&sessions_path);
        let auth = quick_xml::escape::escape(auth_path.as_ref());
        let ssh_authorized_keys = quick_xml::escape::escape(ssh_authorized_keys_path.as_ref());
        format!(
            "    <filesystem type='mount' accessmode='passthrough'>\n      <driver type='virtiofs'/>\n      <source dir='{sessions}'/>\n      <target dir='{}'/>\n    </filesystem>\n    <filesystem type='mount' accessmode='passthrough'>\n      <driver type='virtiofs'/>\n      <source dir='{auth}'/>\n      <target dir='{}'/>\n    </filesystem>\n    <filesystem type='mount' accessmode='passthrough'>\n      <driver type='virtiofs'/>\n      <source dir='{ssh_authorized_keys}'/>\n      <target dir='{}'/>\n    </filesystem>\n",
            crate::CODEX_SESSIONS_TAG,
            crate::CODEX_AUTH_TAG,
            crate::SSH_AUTHORIZED_KEYS_TAG,
        )
    });
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
{interface}{shared_mounts}    <channel type='unix'>
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

fn mac_address(domain_name: &crate::DomainName) -> String {
    let suffix = &domain_name.as_str()[3..];
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
    use crate::{MachineConfig, SharedMounts};
    use std::time::Duration;

    fn test_domain_xml(shared_mounts: Option<SharedMounts>) -> String {
        let world_id = wt_world::WorldId::from(
            uuid::Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap(),
        );
        let domain_name = crate::DomainName::for_world(world_id);
        let config = MachineConfig {
            image: PathBuf::from("/var/lib/wt/images/golden.qcow2"),
            worlds_dir: PathBuf::from("/var/lib/libvirt/images/wt"),
            worlds_owner_uid: 1001,
            network: "default & private".to_owned(),
            boot_timeout: Duration::from_secs(300),
            shared_mounts,
        };
        let spec = crate::MachineSpec {
            world_id,
            memory_mib: 4096,
            vcpus: 4,
            disk_gib: 32,
        };
        domain_xml(
            &domain_name,
            Path::new("/var/lib/libvirt/images/wt/disks/world & head.qcow2"),
            &config,
            &spec,
            true,
        )
    }

    #[test]
    fn domain_without_codex_has_no_virtiofs_support() {
        insta::assert_snapshot!("domain_xml_without_codex", test_domain_xml(None));
    }

    #[test]
    fn domain_with_codex_mounts_has_virtiofs_support() {
        let xml = test_domain_xml(Some(SharedMounts {
            sessions_root: PathBuf::from("/home/wt/.codex/sessions & rollouts"),
            auth: PathBuf::from("/home/wt/.codex/.wt-auth"),
            ssh_authorized_keys: PathBuf::from("/home/wt/.ssh/.wt-authorized-keys"),
        }));
        assert_eq!(xml.matches("accessmode='passthrough'").count(), 3);
        assert!(!xml.contains("<idmap"));
        insta::assert_snapshot!("domain_xml_with_codex_mounts", xml);
    }
}
