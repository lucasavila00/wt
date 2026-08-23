use sha2::{Digest, Sha256};
use wt_workload_registry::Store;

pub(super) fn generation(store: &Store) -> Result<String, String> {
    let mut entries = store
        .list_codex_session_catalog()
        .map_err(|error| error.to_string())?
        .into_iter()
        .collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| {
        left.rollout_path
            .cmp(&right.rollout_path)
            .then(left.session_id.cmp(&right.session_id))
    });
    let mut digest = Sha256::new();
    for entry in entries {
        digest.update(entry.session_id.as_bytes());
        digest.update(entry.rollout_path.as_bytes());
        digest.update(entry.rollout_file_identity.as_bytes());
        digest.update(entry.rollout_length.to_le_bytes());
        digest.update(entry.rollout_modified_at_unix_ns.to_le_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn generation_tracks_shared_sessions_and_rollout_revisions() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("instances.db")).unwrap();
        fs::write(
            temp.path().join("rollout-main.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"11111111-1111-4111-8111-111111111111\",\"source\":{}}}\n",
        )
        .unwrap();
        super::super::refresh_codex_session_catalog(&store, temp.path()).unwrap();
        let first = generation(&store).unwrap();

        fs::write(
            temp.path().join("rollout-main.jsonl"),
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"11111111-1111-4111-8111-111111111111\",\"source\":{}}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\"}}\n"
            ),
        )
        .unwrap();
        super::super::refresh_codex_session_catalog(&store, temp.path()).unwrap();
        let updated = generation(&store).unwrap();
        assert_ne!(updated, first);

        fs::write(
            temp.path().join("rollout-second.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"22222222-2222-4222-8222-222222222222\",\"source\":{}}}\n",
        )
        .unwrap();
        super::super::refresh_codex_session_catalog(&store, temp.path()).unwrap();

        assert_ne!(generation(&store).unwrap(), updated);
        assert_eq!(generation(&store).unwrap().len(), 64);
    }
}
