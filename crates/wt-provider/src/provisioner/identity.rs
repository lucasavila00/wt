use super::WorkerError;
use ssh_key::{HashAlg, PublicKey};
use std::collections::BTreeSet;

pub(super) fn normalized_host_keys(lines: &str) -> BTreeSet<String> {
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

pub(super) fn host_keys_match(expected: &[String], presented: &str) -> bool {
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

pub(super) fn endpoint_identity_error(
    guest_ip: &str,
    expected: &[String],
    presented: &str,
) -> WorkerError {
    let expected = normalized_host_keys(&expected.join("\n"));
    let presented = normalized_host_keys(presented);
    WorkerError::new(format!(
        "SSH endpoint identity mismatch at {guest_ip}:22: expected [{}], presented [{}]. WT refused to publish SSH access because another guest may be using this IP. Inspect the server's DHCP and provider state, remove the stale guest, then run `wt sync`.",
        fingerprints(&expected),
        fingerprints(&presented),
    ))
}
