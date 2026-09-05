---
name: copy-wt-codex-session
description: Copy one retained Codex conversation between WT worlds on the same server and provide the target resume command. Use when a user asks to copy, import, or transfer a Codex thread or rollout between WT worlds; do not use for whole-world cloning or cross-server migration.
---

# Copy a WT Codex session

Copy only the selected Codex rollout file. WT gives each world a server-backed sessions directory keyed by its immutable world ID and mounts that directory at `/home/wt/.codex/sessions` inside the guest. Do not copy Codex databases, indexes, logs, locks, authentication, or WT delivery state.

## Resolve the source and target

Work on the WT server host as the `wt` user.

1. Resolve each user-facing world name to its immutable world ID through the WT control plane. A qualified name such as `ars.curious-fox` contains the client context `ars` and the server-side world name `curious-fox`; do not treat the context as part of the world name.
2. Use the current `PROTOCOL_VERSION` from `crates/products/wt/control-protocol/src/lib.rs` when querying the local `wts api` `list_worlds` operation. Do not infer world IDs from directory order or timestamps.
3. Confirm that both worlds belong to this server and that their session directories are under `/home/wt/.codex/sessions/<world-id>`.

## Select the conversation

Find `.jsonl` rollout files under the source world's directory. If the user supplied a thread ID, select the file whose name and `session_meta.payload.id` match it. Otherwise:

- Select the sole rollout when exactly one exists.
- When several exist, show the thread ID, timestamp, source working directory, and file modification time, then ask the user which conversation to copy. Do not print conversation contents unless requested.

Extract metadata with `jq` from the `session_meta` record. Ensure the thread ID in the metadata matches the UUID in the rollout filename.

Prefer copying an idle conversation. If WT cannot inspect its status, verify the rollout's size, modification time, and SHA-256 digest twice across a short interval. Stop if any value changes; do not copy a file while it is being written.

## Copy safely

Preserve the rollout's path relative to the source world directory so the target receives the same `YYYY/MM/DD/rollout-...jsonl` path.

Before writing:

- Confirm whether the user requested a copy or move. For a copy, never remove or alter the source.
- Check whether the destination path already exists. If it has the same digest, report that the import is already present. If it differs, stop rather than overwrite it.

Create missing dated directories with mode `0700`, then copy the rollout without clobbering an existing file while preserving its mode and timestamps. The resulting rollout must be owned by `wt:wt` and normally have mode `0600`.

A representative copy is:

```bash
sessions_root=/home/wt/.codex/sessions
source_world_id=<source-world-uuid>
target_world_id=<target-world-uuid>
relative_rollout=<YYYY/MM/DD/rollout-file.jsonl>

source_rollout="$sessions_root/$source_world_id/$relative_rollout"
target_rollout="$sessions_root/$target_world_id/$relative_rollout"
target_directory=${target_rollout%/*}

test -f "$source_rollout"
test ! -e "$target_rollout"
install -d -m 700 "$target_directory"
cp --preserve=mode,timestamps --no-clobber "$source_rollout" "$target_rollout"
```

Verify that source and target have identical SHA-256 digests, that the target owner and mode are correct, and that the source still exists for a copy operation.

## Hand off the resumed session

Return the copied thread ID and tell the user to run this inside the target world:

```bash
codex resume <thread-id>
```

If the source working directory also exists in the target, run the command from that directory. If resume fails, preserve both rollouts and report the error. Do not repair or copy Codex's local state database as a workaround.
