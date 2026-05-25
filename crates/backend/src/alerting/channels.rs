//! Notification channel adapters.
//!
//! Each channel kind knows how to turn an `AlertNotificationPayload` into
//! a vendor-specific HTTP request. Channels are stateless — connection
//! pooling / retries belong to the caller.

use std::time::Duration;

use appcontrol_common::alerting::{
    AlertNotificationPayload, AlertSeverity, AlertStatus, NotificationChannelConfig,
};
use serde_json::json;

use super::AlertingError;

/// Trait every channel adapter implements. Dispatch is async and
/// returns `Ok(())` on HTTP 2xx, `Err(_)` otherwise. Caller is expected
/// to log failures into `alert_instances.notifications_sent`.
#[async_trait::async_trait]
pub trait NotificationChannel: Send + Sync {
    async fn dispatch(
        &self,
        client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError>;
}

/// Build a channel adapter from its persisted config row.
pub fn build_channel(config: &NotificationChannelConfig) -> Box<dyn NotificationChannel> {
    match config {
        NotificationChannelConfig::Webhook {
            url,
            secret,
            headers,
        } => Box::new(WebhookChannel {
            url: url.clone(),
            secret: secret.clone(),
            headers: headers.clone(),
        }),
        NotificationChannelConfig::Slack { webhook_url } => Box::new(SlackChannel {
            webhook_url: webhook_url.clone(),
        }),
    }
}

/// Build the shared `reqwest::Client` channels use. Single function so
/// timeouts stay consistent across vendors.
pub fn build_http_client() -> Result<reqwest::Client, AlertingError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("appcontrol-alerting/1.0")
        .build()
        .map_err(|e| AlertingError::Dispatch(format!("client builder: {e}")))
}

// ---------------------------------------------------------------------------
// Generic webhook
// ---------------------------------------------------------------------------

pub struct WebhookChannel {
    url: String,
    secret: Option<String>,
    headers: std::collections::HashMap<String, String>,
}

#[async_trait::async_trait]
impl NotificationChannel for WebhookChannel {
    async fn dispatch(
        &self,
        client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError> {
        let body = serde_json::to_vec(payload)?;

        let mut req = client.post(&self.url).body(body.clone());
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        req = req.header("Content-Type", "application/json");

        // HMAC SHA-256 signature over the raw body so receivers can verify
        // the payload originated from AppControl. Matches the pattern
        // GitHub / Stripe use.
        if let Some(secret) = &self.secret {
            let signature = hmac_sha256_hex(secret.as_bytes(), &body);
            req = req.header("X-AppControl-Signature", format!("sha256={signature}"));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| AlertingError::Dispatch(format!("POST {}: {e}", self.url)))?;

        if !resp.status().is_success() {
            return Err(AlertingError::Dispatch(format!(
                "POST {} returned HTTP {}",
                self.url,
                resp.status()
            )));
        }
        Ok(())
    }
}

fn hmac_sha256_hex(key: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

// ---------------------------------------------------------------------------
// Slack incoming webhook
// ---------------------------------------------------------------------------

pub struct SlackChannel {
    webhook_url: String,
}

#[async_trait::async_trait]
impl NotificationChannel for SlackChannel {
    async fn dispatch(
        &self,
        client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError> {
        let body = slack_body(payload);
        let resp = client
            .post(&self.webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AlertingError::Dispatch(format!("slack POST: {e}")))?;
        if !resp.status().is_success() {
            return Err(AlertingError::Dispatch(format!(
                "slack POST returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

/// Build the Slack incoming-webhook payload (blocks + attachment colour).
/// Public so tests can assert on it without hitting the network.
pub fn slack_body(p: &AlertNotificationPayload) -> serde_json::Value {
    let color = match p.severity {
        AlertSeverity::Info => "#36a64f",
        AlertSeverity::Warning => "#f2c744",
        AlertSeverity::Critical => "#d93f0b",
    };
    let status_emoji = match p.status {
        AlertStatus::Firing => ":rotating_light:",
        AlertStatus::Acknowledged => ":eyes:",
        AlertStatus::Resolved => ":white_check_mark:",
    };
    let title = format!(
        "{} {} — {} ({})",
        status_emoji, p.policy_name, p.component_name, p.severity
    );
    let mut fields = vec![
        json!({"title": "Application", "value": p.app_name, "short": true}),
        json!({"title": "Component", "value": p.component_name, "short": true}),
        json!({"title": "State", "value": p.triggered_state, "short": true}),
        json!({"title": "Severity", "value": format!("{}", p.severity), "short": true}),
    ];
    if let Some(s) = &p.summary {
        fields.push(json!({"title": "Summary", "value": s, "short": false}));
    }
    json!({
        "attachments": [{
            "color": color,
            "title": title,
            "fields": fields,
            "ts": p.fired_at.timestamp(),
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn fixture() -> AlertNotificationPayload {
        AlertNotificationPayload {
            alert_id: Uuid::nil(),
            policy_id: Uuid::nil(),
            policy_name: "Database down".to_string(),
            component_id: Uuid::nil(),
            component_name: "Oracle-DB".to_string(),
            app_id: Uuid::nil(),
            app_name: "Payments".to_string(),
            severity: AlertSeverity::Critical,
            status: AlertStatus::Firing,
            triggered_state: "FAILED".to_string(),
            fired_at: chrono::Utc::now(),
            summary: Some("Oracle process exited".to_string()),
        }
    }

    #[test]
    fn hmac_matches_known_vector() {
        // RFC 4231 test case 1: key=0x0b*20, data="Hi There"
        let key = [0x0b; 20];
        let got = hmac_sha256_hex(&key, b"Hi There");
        assert_eq!(
            got,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn slack_body_has_severity_color_and_status_emoji() {
        let p = fixture();
        let body = slack_body(&p);
        let attachment = &body["attachments"][0];
        assert_eq!(attachment["color"], "#d93f0b"); // critical
        let title = attachment["title"].as_str().unwrap();
        assert!(title.starts_with(":rotating_light:"));
        assert!(title.contains("Oracle-DB"));
        assert!(title.contains("Database down"));
    }

    #[test]
    fn slack_body_includes_summary_field_when_set() {
        let p = fixture();
        let body = slack_body(&p);
        let fields = body["attachments"][0]["fields"].as_array().unwrap();
        assert!(fields
            .iter()
            .any(|f| f["title"] == "Summary" && f["value"] == "Oracle process exited"));
    }

    #[test]
    fn slack_body_omits_summary_when_none() {
        let mut p = fixture();
        p.summary = None;
        let body = slack_body(&p);
        let fields = body["attachments"][0]["fields"].as_array().unwrap();
        assert!(fields.iter().all(|f| f["title"] != "Summary"));
    }

    #[test]
    fn slack_body_uses_warning_color() {
        let mut p = fixture();
        p.severity = AlertSeverity::Warning;
        let body = slack_body(&p);
        assert_eq!(body["attachments"][0]["color"], "#f2c744");
    }

    #[test]
    fn slack_body_uses_resolved_emoji() {
        let mut p = fixture();
        p.status = AlertStatus::Resolved;
        let body = slack_body(&p);
        let title = body["attachments"][0]["title"].as_str().unwrap();
        assert!(title.starts_with(":white_check_mark:"));
    }

    #[tokio::test]
    async fn webhook_includes_hmac_signature_when_secret_set() {
        // Spin up a tiny in-process HTTP server that captures the request.
        use std::net::SocketAddr;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let captured: Arc<Mutex<Option<(String, Vec<u8>)>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(
                |headers: axum::http::HeaderMap, body: axum::body::Bytes| async move {
                    let sig = headers
                        .get("X-AppControl-Signature")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    *captured_clone.lock().await = Some((sig, body.to_vec()));
                    axum::http::StatusCode::OK
                },
            ),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let cfg = NotificationChannelConfig::Webhook {
            url: format!("http://{addr}/hook"),
            secret: Some("topsecret".to_string()),
            headers: Default::default(),
        };
        let channel = build_channel(&cfg);
        let client = build_http_client().unwrap();
        let p = fixture();

        channel.dispatch(&client, &p).await.unwrap();

        let (sig, body) = captured
            .lock()
            .await
            .clone()
            .expect("server should have captured the request");
        assert!(sig.starts_with("sha256="));
        let expected = format!("sha256={}", hmac_sha256_hex(b"topsecret", &body));
        assert_eq!(sig, expected);
    }

    #[tokio::test]
    async fn webhook_propagates_http_failure() {
        use std::net::SocketAddr;
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(|| async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let cfg = NotificationChannelConfig::Webhook {
            url: format!("http://{addr}/hook"),
            secret: None,
            headers: Default::default(),
        };
        let channel = build_channel(&cfg);
        let client = build_http_client().unwrap();
        let res = channel.dispatch(&client, &fixture()).await;
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("HTTP 500"), "got: {err}");
    }
}
