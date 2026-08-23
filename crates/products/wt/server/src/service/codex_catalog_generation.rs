use sha2::{Digest, Sha256};
use wt_workload_registry::Store;

pub(super) fn generation(store: &Store) -> Result<String, String> {
    let mut session_ids = store
        .list_codex_session_catalog()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| entry.session_id)
        .collect::<Vec<_>>();
    session_ids.sort_unstable();
    let mut digest = Sha256::new();
    for session_id in session_ids {
        digest.update(session_id.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn generation_tracks_the_set_of_shared_sessions() {
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
            temp.path().join("rollout-second.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"22222222-2222-4222-8222-222222222222\",\"source\":{}}}\n",
        )
        .unwrap();
        super::super::refresh_codex_session_catalog(&store, temp.path()).unwrap();

        assert_ne!(generation(&store).unwrap(), first);
        assert_eq!(generation(&store).unwrap().len(), 64);
    }
}
