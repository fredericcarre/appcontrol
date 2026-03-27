-- V043: Component Log Sources for diagnostics and monitoring
-- Allows declaring log files, Windows Event Log sources, and diagnostic commands

-- Log sources table
CREATE TABLE component_log_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    organization_id UUID NOT NULL REFERENCES organizations(id),

    -- Source identification
    name VARCHAR(100) NOT NULL,
    source_type VARCHAR(20) NOT NULL CHECK (source_type IN ('file', 'event_log', 'command')),

    -- For type 'file': path to log file (supports wildcards)
    file_path TEXT,

    -- For type 'event_log': Windows Event Log settings
    event_log_name VARCHAR(50),      -- 'Application', 'System', 'Security'
    event_log_source VARCHAR(100),   -- Source filter (e.g., 'CRM', 'RabbitMQ')
    event_log_level VARCHAR(50),     -- 'Error', 'Warning', 'Information' (comma-separated)

    -- For type 'command': diagnostic command to execute
    command TEXT,
    command_timeout_seconds INT DEFAULT 30,

    -- Access control and limits
    max_lines INT DEFAULT 1000,
    max_age_hours INT DEFAULT 24,
    is_sensitive BOOLEAN DEFAULT false,  -- Requires higher permission

    -- Display
    display_order INT DEFAULT 0,
    description TEXT,

    -- Audit
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_log_sources_component ON component_log_sources(component_id);
CREATE INDEX idx_log_sources_org ON component_log_sources(organization_id);

-- Add process capture settings to components
ALTER TABLE components ADD COLUMN IF NOT EXISTS log_capture_enabled BOOLEAN DEFAULT true;
ALTER TABLE components ADD COLUMN IF NOT EXISTS log_buffer_lines INT DEFAULT 10000;

-- Log access audit table (tracks who accessed what logs)
CREATE TABLE log_access_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID REFERENCES users(id),
    component_id UUID REFERENCES components(id),
    log_source_id UUID REFERENCES component_log_sources(id),

    source_type VARCHAR(20) NOT NULL,  -- 'process', 'file', 'event_log', 'command'
    source_name VARCHAR(100),

    -- What was requested
    lines_requested INT,
    filter_applied TEXT,
    time_range_hours INT,

    -- Access details
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip_address INET,
    user_agent TEXT
);

CREATE INDEX idx_log_audit_component ON log_access_audit(component_id);
CREATE INDEX idx_log_audit_user ON log_access_audit(user_id);
CREATE INDEX idx_log_audit_time ON log_access_audit(accessed_at);
