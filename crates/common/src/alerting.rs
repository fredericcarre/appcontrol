//! Shared types for the alerting layer.
//!
//! The backend owns the engine that watches FSM transitions, but the
//! channel-config shape and the public-facing alert payloads live here so
//! the CLI and API clients can deserialize them without pulling in the
//! whole backend.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AlertStatus {
    Firing,
    Acknowledged,
    Resolved,
}

/// Selector describes which components an alert policy applies to. Empty
/// (`{}`) matches every component the user can see. Fields combine with
/// AND semantics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertSelector {
    /// Match only components belonging to this application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<Uuid>,
    /// Match exactly this component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<Uuid>,
    /// Match components whose `tags` JSONB contains all of these key/value
    /// pairs. Useful for tier-wide policies (`env=prod`, `tier=database`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
}

/// Vendor-specific config for a notification channel. Stored in the
/// `notification_channels.config` JSONB column; the `kind` discriminator
/// (also persisted) decides which variant the engine deserializes into.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationChannelConfig {
    /// Generic outbound webhook. POSTs a JSON payload. The `secret`, if
    /// set, is sent as `X-AppControl-Signature: sha256=<hex>` over the
    /// raw body so the receiver can authenticate AppControl.
    Webhook {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<String>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        headers: HashMap<String, String>,
    },
    /// Slack incoming webhook. Same transport as `Webhook` but the
    /// payload is shaped as Slack message blocks so it renders nicely in
    /// channel.
    Slack { webhook_url: String },
}

impl NotificationChannelConfig {
    /// Return a copy where secret-bearing fields are masked. Use this when
    /// serialising for API responses, audit logs, or anywhere an operator
    /// can read the value back.
    pub fn redacted(&self) -> Self {
        match self {
            NotificationChannelConfig::Webhook {
                url,
                secret,
                headers,
            } => NotificationChannelConfig::Webhook {
                url: url.clone(),
                secret: secret.as_ref().map(|_| "***".to_string()),
                headers: headers.clone(),
            },
            NotificationChannelConfig::Slack { webhook_url } => {
                NotificationChannelConfig::Slack {
                    // The Slack webhook URL embeds the credential — mask it.
                    webhook_url: redact_slack_url(webhook_url),
                }
            }
        }
    }
}

fn redact_slack_url(url: &str) -> String {
    // Slack incoming-webhook URLs end with a path segment that is the
    // bot's per-channel credential. We mask everything after the last
    // slash, leaving the host + service path visible for debugging.
    match url.rsplit_once('/') {
        Some((prefix, _secret)) => format!("{prefix}/***"),
        None => "***".to_string(),
    }
}

/// Payload pushed to a notification channel when an alert fires.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertNotificationPayload {
    pub alert_id: Uuid,
    pub policy_id: Uuid,
    pub policy_name: String,
    pub component_id: Uuid,
    pub component_name: String,
    pub app_id: Uuid,
    pub app_name: String,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub triggered_state: String,
    pub fired_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_serializes_lowercase() {
        let json = serde_json::to_string(&AlertSeverity::Critical).unwrap();
        assert_eq!(json, "\"critical\"");
    }

    #[test]
    fn selector_empty_is_default() {
        let s = AlertSelector::default();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn webhook_redacts_secret() {
        let c = NotificationChannelConfig::Webhook {
            url: "https://example.com/hook".into(),
            secret: Some("super-secret".into()),
            headers: HashMap::new(),
        };
        let r = c.redacted();
        match r {
            NotificationChannelConfig::Webhook { secret, .. } => {
                assert_eq!(secret.as_deref(), Some("***"));
            }
            _ => panic!("kind changed"),
        }
    }

    #[test]
    fn slack_url_secret_is_masked() {
        // Synthetic URL (example.invalid host) so the test doesn't trip
        // any vendor-specific secret scanner. The redaction logic only
        // looks at the last path segment regardless of host.
        let c = NotificationChannelConfig::Slack {
            webhook_url: "https://hooks.example.invalid/services/team/bot/per-channel-credential"
                .to_string(),
        };
        match c.redacted() {
            NotificationChannelConfig::Slack { webhook_url } => {
                assert!(!webhook_url.contains("per-channel-credential"));
                assert!(webhook_url.ends_with("/***"));
            }
            _ => panic!("kind changed"),
        }
    }

    #[test]
    fn channel_config_roundtrips_through_serde() {
        let original = NotificationChannelConfig::Webhook {
            url: "https://x".into(),
            secret: Some("s".into()),
            headers: [("X-Trace".to_string(), "1".to_string())]
                .into_iter()
                .collect(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: NotificationChannelConfig = serde_json::from_str(&json).unwrap();
        match back {
            NotificationChannelConfig::Webhook {
                url,
                secret,
                headers,
            } => {
                assert_eq!(url, "https://x");
                assert_eq!(secret.as_deref(), Some("s"));
                assert_eq!(headers["X-Trace"], "1");
            }
            _ => panic!("kind changed"),
        }
    }
}
