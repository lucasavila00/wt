use super::*;

impl MachineProvider for LibvirtProvider {
    fn create(&self, spec: &MachineSpec, progress: &mut dyn Write) -> Result<Machine, WorkerError> {
        match self.create_inner(spec, progress) {
            Ok(machine) => Ok(machine),
            Err(primary) => {
                if let Err(cleanup) = self.cleanup(&spec.provider_id, &[spec.disk_id]) {
                    Err(WorkerError::new(format!(
                        "{primary} (cleanup also failed: {cleanup})"
                    )))
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn fork(&self, spec: &ForkMachineSpec, progress: &mut dyn Write) -> Result<Machine, ForkError> {
        self.fork_inner(spec, progress)
    }

    fn inspect(&self, provider_id: &ProviderId) -> Result<MachineInspection, WorkerError> {
        let directory = self.config.worlds_dir.join(provider_id.as_str());
        let connection = LibvirtConnection::open()?;
        let domain = match Domain::lookup_by_name(&connection, provider_id.as_str()) {
            Ok(domain) => Some(domain),
            Err(error) if error.code() == ErrorNumber::NoDomain => None,
            Err(error) => return Err(context("look up libvirt domain", error)),
        };
        match (domain, directory.exists()) {
            (None, false) => Ok(MachineInspection::Missing),
            (None, true) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: files exist but domain is missing",
                provider_id
            ))),
            (Some(_), false) => Err(WorkerError::new(format!(
                "partial libvirt machine {}: domain exists but files are missing",
                provider_id
            ))),
            (Some(domain), true) => {
                let paths = world::Paths::new(&self.config.worlds_dir, provider_id);
                if [
                    &paths.seed,
                    &paths.user_data,
                    &paths.meta_data,
                    &paths.network_config,
                ]
                .into_iter()
                .any(|path| !path.is_file())
                {
                    return Err(WorkerError::new(format!(
                        "partial libvirt machine {provider_id}: required machine files are missing"
                    )));
                }
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
                let guest_ip = domain_ip(provider_id)?.ok_or_else(|| {
                    WorkerError::new(format!("libvirt machine {provider_id} has no IPv4 address"))
                })?;
                Ok(MachineInspection::Running(
                    self.machine(provider_id, guest_ip),
                ))
            }
        }
    }

    fn start(&self, provider_id: &ProviderId) -> Result<Machine, WorkerError> {
        match self.inspect(provider_id)? {
            MachineInspection::Missing => {
                return Err(WorkerError::new(format!(
                    "libvirt machine {provider_id} is missing"
                )))
            }
            MachineInspection::Running(machine) => return Ok(machine),
            MachineInspection::Stopped { .. } => {}
        }
        let domain = lookup_domain(provider_id)?;
        domain
            .create()
            .map_err(|error| context("start KVM domain", error))?;
        self.wait_for_agent(provider_id)?;
        let guest_ip = self.wait_for_ip(provider_id)?;
        Ok(self.machine(provider_id, guest_ip))
    }

    fn delete(&self, provider_id: &ProviderId, disk_ids: &[uuid::Uuid]) -> Result<(), WorkerError> {
        self.cleanup(provider_id, disk_ids)
    }
}
