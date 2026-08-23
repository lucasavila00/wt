use super::*;

impl MachineProvider for LibvirtProvider {
    fn image_path(&self) -> &std::path::Path {
        &self.config.image
    }

    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError> {
        match self.create_inner(spec, progress) {
            Ok(machine) => Ok(machine),
            Err(primary) => {
                if let Err(cleanup) = self.cleanup(spec.world_id) {
                    Err(WorkerError::new(format!(
                        "{primary} (cleanup also failed: {cleanup})"
                    )))
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn inspect(&self, world_id: WorldId) -> Result<MachineInspection, WorkerError> {
        let domain_name = DomainName::for_world(world_id);
        let directory = self.config.worlds_dir.join(domain_name.as_str());
        let connection = LibvirtConnection::open()?;
        let domain = match Domain::lookup_by_name(&connection, domain_name.as_str()) {
            Ok(domain) => Some(domain),
            Err(error) if error.code() == ErrorNumber::NoDomain => None,
            Err(error) => return Err(context("look up libvirt domain", error)),
        };
        match (domain, directory.exists()) {
            (None, false) => Ok(MachineInspection::Missing),
            (None, true) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: files exist but domain is missing",
                domain_name
            ))),
            (Some(_), false) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: domain exists but files are missing",
                domain_name
            ))),
            (Some(domain), true) => {
                if !domain
                    .is_active()
                    .map_err(|error| context("check domain state", error))?
                {
                    let (_, reason) = domain
                        .get_state()
                        .map_err(|error| context("read stopped domain state", error))?;
                    return Ok(MachineInspection::Stopped {
                        reason: shutdown_reason(reason).map(str::to_owned),
                    });
                }
                domain
                    .qemu_agent_command(r#"{"execute":"guest-ping"}"#, 5, 0)
                    .map_err(|error| context("contact QEMU guest agent", error))?;
                let guest_ip = domain_ip(&domain_name)?.ok_or_else(|| {
                    WorkerError::new(format!("libvirt machine {domain_name} has no IPv4 address"))
                })?;
                Ok(MachineInspection::Running(self.machine(world_id, guest_ip)))
            }
        }
    }

    fn start(&self, world_id: WorldId) -> Result<Machine, WorkerError> {
        let domain_name = DomainName::for_world(world_id);
        match self.inspect(world_id)? {
            MachineInspection::Missing => {
                return Err(WorkerError::new(format!(
                    "libvirt machine {domain_name} is missing"
                )))
            }
            MachineInspection::Running(machine) => return Ok(machine),
            MachineInspection::Stopped { .. } => {}
        }
        let domain = lookup_domain(&domain_name)?;
        domain
            .create()
            .map_err(|error| context("start KVM domain", error))?;
        self.wait_for_agent(&domain_name)?;
        let guest_ip = self.wait_for_ip(&domain_name)?;
        Ok(self.machine(world_id, guest_ip))
    }

    fn stop(&self, world_id: WorldId) -> Result<(), WorkerError> {
        let domain_name = DomainName::for_world(world_id);
        let domain = lookup_domain(&domain_name)?;
        if !domain
            .is_active()
            .map_err(|error| context("check domain state", error))?
        {
            return Ok(());
        }
        domain
            .shutdown_flags(virt::sys::VIR_DOMAIN_SHUTDOWN_GUEST_AGENT)
            .map_err(|error| context("request guest shutdown", error))?;
        let deadline = std::time::Instant::now() + self.config.boot_timeout;
        loop {
            if !domain
                .is_active()
                .map_err(|error| context("check domain shutdown state", error))?
            {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(WorkerError::new(format!(
                    "timed out waiting for domain {domain_name} to shut down"
                )));
            }
            std::thread::sleep(SHUTDOWN_POLL_INTERVAL);
        }
    }

    fn disk_usage(&self, world_id: WorldId) -> Result<u64, WorkerError> {
        self.allocated_disk_bytes(world_id)
    }

    fn delete(&self, world_id: WorldId) -> Result<(), WorkerError> {
        self.cleanup(world_id)
    }
}
