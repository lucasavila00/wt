# wt-workload-registry

SQLite world registry and host-capacity admission.

It owns migrations, world and disk records, latest per-world Codex session
observations, and atomic CPU, RAM, and disk reservations. Lifecycle logic stays
in `wt-server`.
