use virt::connect::Connect;
use virt::network::Network;
use wt_provider::{ProviderId, WorkerError};

use super::{context, lookup_domain};

pub(super) fn domain_ip(provider_id: &ProviderId) -> Result<Option<String>, WorkerError> {
    let domain = lookup_domain(provider_id)?;
    let interfaces = domain
        .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
        .map_err(|error| context("get domain interface addresses", error))?;
    Ok(interfaces
        .into_iter()
        .flat_map(|interface| interface.addrs)
        .find_map(|address| {
            let ip = address.addr.parse::<std::net::IpAddr>().ok()?;
            (ip.is_ipv4() && !ip.is_loopback()).then(|| ip.to_string())
        }))
}

pub(super) fn network_address(connection: &Connect, name: &str) -> Result<String, WorkerError> {
    let network = Network::lookup_by_name(connection, name)
        .map_err(|error| context("look up libvirt network", error))?;
    let xml = network
        .get_xml_desc(0)
        .map_err(|error| context("read libvirt network XML", error))?;
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
