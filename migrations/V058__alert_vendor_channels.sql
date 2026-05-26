-- V058: Add vendor channel adapters (email, PagerDuty, MS Teams).
--
-- V057 shipped two adapters (webhook, slack). This sprint adds the
-- three most-requested production destinations:
--   * email   — for human inbox alerting (works with any SMTP server)
--   * pagerduty — for on-call paging via PD Events API v2
--   * teams   — for Microsoft Teams channels via incoming webhooks
--
-- The change is a single CHECK-constraint widening on the kind column;
-- no schema otherwise (vendor-specific fields live in the existing
-- `config` JSONB).

ALTER TABLE notification_channels
    DROP CONSTRAINT IF EXISTS notification_channels_kind_check;

ALTER TABLE notification_channels
    ADD CONSTRAINT notification_channels_kind_check
    CHECK (kind IN ('webhook', 'slack', 'email', 'pagerduty', 'teams'));
