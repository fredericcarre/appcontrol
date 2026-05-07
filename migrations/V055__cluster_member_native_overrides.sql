-- V055: Per-member native command overrides for fan-out clusters.
--
-- v1.18.0 (V053) added typed native commands at the component level. v1.18.1
-- left fan-out members on shell-only because the URL/host of an HTTP probe
-- has to differ per member. This migration finishes the picture:
--
--   * Three new JSONB columns on cluster_members. NULL = "inherit from the
--     parent component", same precedence rule as the existing shell
--     `*_cmd_override` siblings.
--   * The agent runs whichever NativeCommand it ends up with through a
--     small templater that swaps `{hostname}` / `{install_path}` for the
--     member's actual values, so a single parent `check_native` like
--     `https://{hostname}:8443/health` fans out across the tier without
--     anyone copy-pasting JSON per member.
--
-- Same precedence on the agent side, top wins:
--   1. cluster_members.{check_native_override, ...}     — explicit per member
--   2. components.{check_native, ...}                   — templated per member
--   3. cluster_members.{check_cmd_override, ...}        — shell, per member
--   4. components.{check_cmd, ...}                      — shell, parent default

ALTER TABLE cluster_members
    ADD COLUMN IF NOT EXISTS check_native_override JSONB,
    ADD COLUMN IF NOT EXISTS start_native_override JSONB,
    ADD COLUMN IF NOT EXISTS stop_native_override JSONB;
