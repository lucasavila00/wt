# wt-registry

Shared SQLite guest registry and host-capacity admission.

It owns migrations, common guest and disk records, GitHub CI runner records,
and atomic CPU, RAM, and disk reservations. Retained-world storage and lifecycle
logic stays in `wt-server`.

Storage model: [Database](../../docs/internals/database.md).
