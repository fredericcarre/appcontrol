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
    /// SMTP email. Connects to any RFC 5321 server. Use Tls or StartTls
    /// in production; `None` is provided only for trusted localhost
    /// relays.
    Email {
        smtp_host: String,
        #[serde(default = "default_smtp_port")]
        smtp_port: u16,
        smtp_user: String,
        smtp_password: String,
        /// `From:` header — must usually be an address the SMTP server is
        /// authorised to relay for.
        from: String,
        /// One or more recipients. Each entry is a single RFC 5322 address.
        to: Vec<String>,
        #[serde(default)]
        tls: EmailTls,
    },
    /// PagerDuty Events API v2. The `routing_key` is the per-service
    /// integration key (Event Routing → "Integration Key"); it
    /// authenticates AppControl as the event source. Alerts dedupe on the
    /// AppControl alert fingerprint so a repeated firing reuses the same
    /// PagerDuty incident.
    PagerDuty { routing_key: String },
    /// Microsoft Teams incoming webhook (Connector). Sends an adaptive
    /// card so the message renders with the AppControl branding /
    /// severity colour in the Teams channel.
    Teams { webhook_url: String },
}

/// TLS profile for SMTP. `Tls` connects on a TLS-wrapped socket
/// (port 465), `StartTls` upgrades a plaintext connection (port 587),
/// `None` is plaintext (port 25 / localhost relays).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailTls {
    None,
    StartTls,
    #[default]
    Tls,
}

fn default_smtp_port() -> u16 {
    587
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
                    webhook_url: redact_url_last_segment(webhook_url),
                }
            }
            NotificationChannelConfig::Email {
                smtp_host,
                smtp_port,
                smtp_user,
                smtp_password: _,
                from,
                to,
                tls,
            } => NotificationChannelConfig::Email {
                smtp_host: smtp_host.clone(),
                smtp_port: *smtp_port,
                smtp_user: smtp_user.clone(),
                smtp_password: "***".to_string(),
                from: from.clone(),
                to: to.clone(),
                tls: *tls,
            },
            NotificationChannelConfig::PagerDuty { routing_key: _ } => {
                NotificationChannelConfig::PagerDuty {
                    routing_key: "***".to_string(),
                }
            }
            NotificationChannelConfig::Teams { webhook_url } => NotificationChannelConfig::Teams {
                webhook_url: redact_url_last_segment(webhook_url),
            },
        }
    }
}

/// Mask the last path segment of a URL. Used for Slack and Teams
/// incoming webhooks, whose URLs embed a per-destination credential as
/// the final path component. The host + leading path stay visible for
/// debugging.
fn redact_url_last_segment(url: &str) -> String {
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
    fn email_redacts_password_only() {
        let c = NotificationChannelConfig::Email {
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_user: "ops@example.com".into(),
            smtp_password: "super-secret".into(),
            from: "appcontrol@example.com".into(),
            to: vec!["oncall@example.com".into()],
            tls: EmailTls::StartTls,
        };
        match c.redacted() {
            NotificationChannelConfig::Email {
                smtp_user,
                smtp_password,
                from,
                to,
                ..
            } => {
                assert_eq!(smtp_password, "***");
                // Non-secret fields preserved.
                assert_eq!(smtp_user, "ops@example.com");
                assert_eq!(from, "appcontrol@example.com");
                assert_eq!(to, vec!["oncall@example.com".to_string()]);
            }
            _ => panic!("kind changed"),
        }
    }

    #[test]
    fn pagerduty_redacts_routing_key() {
        let c = NotificationChannelConfig::PagerDuty {
            routing_key: "R0UTINGK3Y-not-real".into(),
        };
        match c.redacted() {
            NotificationChannelConfig::PagerDuty { routing_key } => {
                assert_eq!(routing_key, "***");
            }
            _ => panic!("kind changed"),
        }
    }

    #[test]
    fn teams_redacts_webhook_token() {
        let c = NotificationChannelConfig::Teams {
            webhook_url: "https://outlook.office.com/webhook/abc/def/token-tail".into(),
        };
        match c.redacted() {
            NotificationChannelConfig::Teams { webhook_url } => {
                assert!(!webhook_url.contains("token-tail"));
                assert!(webhook_url.ends_with("/***"));
            }
            _ => panic!("kind changed"),
        }
    }

    #[test]
    fn email_default_tls_is_tls() {
        let json = r#"{
            "kind":"email","smtp_host":"x","smtp_user":"u","smtp_password":"p",
            "from":"a","to":["b"]
        }"#;
        let c: NotificationChannelConfig = serde_json::from_str(json).unwrap();
        match c {
            NotificationChannelConfig::Email { tls, smtp_port, .. } => {
                assert_eq!(tls, EmailTls::Tls);
                assert_eq!(smtp_port, 587);
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
