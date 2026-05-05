-- V053: Native command specs (HTTP / TCP / process)
--
-- Lets components describe their check / start / stop operations as a
-- typed payload (e.g. an HTTP probe with method/url/expect_status) instead
-- of a shell command — useful for environments that don't have curl,
-- wget, or a comparable utility on every host (Windows is the typical
-- pain point).
--
-- We keep the existing `*_cmd` TEXT columns for shell commands. When a
-- native spec is present, the agent runs it instead of the shell command.
-- That keeps imports backward-compatible: every existing row still has
-- check_native = NULL and behaves exactly as before.
--
-- Spec shape (frontend + backend agree on JSON):
--   { "kind": "http",
--     "method": "GET",
--     "url": "http://localhost:8080/health",
--     "expect_status": 200,
--     "timeout_seconds": 5,
--     "headers": { "Authorization": "Bearer …" },   -- optional
--     "body": "…",                                  -- optional
--     "insecure": false                              -- optional
--   }
--
-- Other kinds (tcp, process) will reuse the same column with a different
-- "kind" discriminator — no further migrations needed.

ALTER TABLE components
    ADD COLUMN IF NOT EXISTS check_native JSONB,
    ADD COLUMN IF NOT EXISTS start_native JSONB,
    ADD COLUMN IF NOT EXISTS stop_native  JSONB;
