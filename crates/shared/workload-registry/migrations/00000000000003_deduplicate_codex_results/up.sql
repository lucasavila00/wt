CREATE TABLE codex_result_deliveries (
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    mail_id BIGINT NOT NULL REFERENCES world_mail(id) ON DELETE CASCADE,
    PRIMARY KEY (world_id, thread_id, turn_id)
);

-- Preserve the first already-recorded result for each turn. Ordinary mail is unchanged.
INSERT OR IGNORE INTO codex_result_deliveries (world_id, thread_id, turn_id, mail_id)
SELECT world_id,
       json_extract(substr(message, 20), '$.thread_id'),
       json_extract(substr(message, 20), '$.turn_id'),
       id
FROM world_mail
WHERE substr(message, 1, 19) = 'WT_CODEX_RESULT_V2:'
  AND CASE WHEN json_valid(substr(message, 20)) THEN
      json_extract(substr(message, 20), '$.kind') IN ('completed', 'failed')
      AND json_type(substr(message, 20), '$.thread_id') = 'text'
      AND json_type(substr(message, 20), '$.turn_id') = 'text'
  ELSE 0 END
ORDER BY id;
