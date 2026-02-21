-- V008: Materialized Views and Indexes for Reporting

CREATE MATERIALIZED VIEW component_daily_stats AS
SELECT
    st.component_id,
    date_trunc('day', st.created_at)::date AS date,
    COUNT(*) FILTER (WHERE st.to_state = 'RUNNING') AS running_transitions,
    COUNT(*) FILTER (WHERE st.to_state = 'FAILED') AS failed_transitions,
    COUNT(*) FILTER (WHERE st.to_state = 'STOPPED') AS stopped_transitions,
    -- Approximate running seconds (count transitions to RUNNING * avg interval)
    COUNT(*) FILTER (WHERE st.to_state = 'RUNNING') * 30 AS running_seconds,
    86400 AS total_seconds,
    COUNT(*) AS total_transitions
FROM state_transitions st
GROUP BY st.component_id, date_trunc('day', st.created_at)::date;

CREATE UNIQUE INDEX idx_component_daily_stats_pk ON component_daily_stats (component_id, date);
CREATE INDEX idx_component_daily_stats_date ON component_daily_stats (date);

-- Refresh function (to be called periodically by backend)
-- REFRESH MATERIALIZED VIEW CONCURRENTLY component_daily_stats;
