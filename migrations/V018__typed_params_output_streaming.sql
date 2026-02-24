-- V018: Typed command parameters and output streaming support
--
-- 1. Add param_type and enum_values to command_input_params
--    Supports: string, number, boolean, enum, date, password
-- 2. Add user_id to command_executions for audit
-- 3. Add command_text to command_executions (record what was actually run)

-- ============================================================
-- 1. Typed Parameters
-- ============================================================

ALTER TABLE command_input_params
    ADD COLUMN param_type VARCHAR(20) NOT NULL DEFAULT 'string'
        CHECK (param_type IN ('string', 'number', 'boolean', 'enum', 'date', 'password')),
    ADD COLUMN enum_values JSONB;

-- ============================================================
-- 2. Enhanced command_executions
-- ============================================================

ALTER TABLE command_executions
    ADD COLUMN IF NOT EXISTS user_id UUID,
    ADD COLUMN IF NOT EXISTS command_text TEXT;
