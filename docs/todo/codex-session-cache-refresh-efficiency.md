# Avoid rewriting every cached Codex session on every refresh

The catalog parses only new rollout bytes, but the two-second worker and each
inventory request still walk every rollout path and upsert unchanged rows.

Make the background worker the normal refresh owner, write only entries whose
file identity, length, or complete-record offset changed, and let inventory
reads report cache freshness instead of repeating the full refresh. Preserve a
bounded recovery path when the worker has not warmed the catalog.
