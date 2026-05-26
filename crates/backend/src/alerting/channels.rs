//! Notification channel adapters.
//!
//! Each channel kind knows how to turn an `AlertNotificationPayload` into
//! a vendor-specific HTTP request. Channels are stateless — connection
//! pooling / retries belong to the caller.

use std::time::Duration;

use appcontrol_common::alerting::{
    AlertNotificationPayload, AlertSeverity, AlertStatus, EmailTls, NotificationChannelConfig,
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
        NotificationChannelConfig::Email {
            smtp_host,
            smtp_port,
            smtp_user,
            smtp_password,
            from,
            to,
            tls,
        } => Box::new(EmailChannel {
            smtp_host: smtp_host.clone(),
            smtp_port: *smtp_port,
            smtp_user: smtp_user.clone(),
            smtp_password: smtp_password.clone(),
            from: from.clone(),
            to: to.clone(),
            tls: *tls,
        }),
        NotificationChannelConfig::PagerDuty { routing_key } => Box::new(PagerDutyChannel {
            routing_key: routing_key.clone(),
            api_url: "https://events.pagerduty.com/v2/enqueue".to_string(),
        }),
        NotificationChannelConfig::Teams { webhook_url } => Box::new(TeamsChannel {
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

// ---------------------------------------------------------------------------
// PagerDuty Events API v2
// ---------------------------------------------------------------------------

pub struct PagerDutyChannel {
    routing_key: String,
    api_url: String,
}

#[async_trait::async_trait]
impl NotificationChannel for PagerDutyChannel {
    async fn dispatch(
        &self,
        client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError> {
        let body = pagerduty_body(&self.routing_key, payload);
        let resp = client
            .post(&self.api_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AlertingError::Dispatch(format!("pagerduty POST: {e}")))?;
        if !resp.status().is_success() {
            return Err(AlertingError::Dispatch(format!(
                "pagerduty POST returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

/// Build the PagerDuty Events API v2 payload. Public so tests can verify
/// the shape without hitting the live API.
///
/// Lifecycle mapping:
///   * AppControl Firing      → PD event_action="trigger"
///   * AppControl Acknowledged → PD event_action="acknowledge"
///   * AppControl Resolved    → PD event_action="resolve"
///
/// The AppControl alert UUID becomes PagerDuty's dedup_key so the same
/// incident is reused across firing → ack → resolve.
pub fn pagerduty_body(routing_key: &str, p: &AlertNotificationPayload) -> serde_json::Value {
    let event_action = match p.status {
        AlertStatus::Firing => "trigger",
        AlertStatus::Acknowledged => "acknowledge",
        AlertStatus::Resolved => "resolve",
    };
    let severity = match p.severity {
        AlertSeverity::Info => "info",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    };
    let summary = p
        .summary
        .clone()
        .unwrap_or_else(|| format!("{} on {}", p.policy_name, p.component_name));

    json!({
        "routing_key": routing_key,
        "event_action": event_action,
        "dedup_key": p.alert_id.to_string(),
        "payload": {
            "summary": summary,
            "source": p.component_name,
            "severity": severity,
            "component": p.component_name,
            "group": p.app_name,
            "class": p.triggered_state,
            "custom_details": {
                "policy_name":     p.policy_name,
                "policy_id":       p.policy_id.to_string(),
                "component_id":    p.component_id.to_string(),
                "app_id":          p.app_id.to_string(),
                "triggered_state": p.triggered_state,
                "fired_at":        p.fired_at,
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Microsoft Teams incoming webhook (adaptive card)
// ---------------------------------------------------------------------------

pub struct TeamsChannel {
    webhook_url: String,
}

#[async_trait::async_trait]
impl NotificationChannel for TeamsChannel {
    async fn dispatch(
        &self,
        client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError> {
        let body = teams_body(payload);
        let resp = client
            .post(&self.webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AlertingError::Dispatch(format!("teams POST: {e}")))?;
        if !resp.status().is_success() {
            return Err(AlertingError::Dispatch(format!(
                "teams POST returned HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

/// MS Teams "MessageCard" (the simpler legacy format, more widely
/// accepted by incoming-webhook connectors than the newer Adaptive
/// Card schema). Includes a coloured theme bar driven by severity and
/// a fact list with the alert metadata.
pub fn teams_body(p: &AlertNotificationPayload) -> serde_json::Value {
    let theme = match p.severity {
        AlertSeverity::Info => "36a64f",
        AlertSeverity::Warning => "f2c744",
        AlertSeverity::Critical => "d93f0b",
    };
    let title = format!("{} — {} ({})", p.policy_name, p.component_name, p.severity);
    let mut facts = vec![
        json!({"name": "Application", "value": p.app_name}),
        json!({"name": "Component",   "value": p.component_name}),
        json!({"name": "State",       "value": p.triggered_state}),
        json!({"name": "Severity",    "value": format!("{}", p.severity)}),
        json!({"name": "Status",      "value": format!("{}", p.status)}),
        json!({"name": "Fired at",    "value": p.fired_at.to_rfc3339()}),
    ];
    if let Some(s) = &p.summary {
        facts.push(json!({"name": "Summary", "value": s}));
    }
    json!({
        "@type": "MessageCard",
        "@context": "https://schema.org/extensions",
        "themeColor": theme,
        "summary": title,
        "title": title,
        "sections": [{
            "facts": facts,
            "markdown": true,
        }]
    })
}

// ---------------------------------------------------------------------------
// Email (SMTP via lettre)
// ---------------------------------------------------------------------------

pub struct EmailChannel {
    smtp_host: String,
    smtp_port: u16,
    smtp_user: String,
    smtp_password: String,
    from: String,
    to: Vec<String>,
    tls: EmailTls,
}

#[async_trait::async_trait]
impl NotificationChannel for EmailChannel {
    async fn dispatch(
        &self,
        _client: &reqwest::Client,
        payload: &AlertNotificationPayload,
    ) -> Result<(), AlertingError> {
        use lettre::message::{header::ContentType, Message};
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::transport::smtp::AsyncSmtpTransport;
        use lettre::{AsyncTransport, Tokio1Executor};

        let (subject, body) = email_subject_and_body(payload);

        let from = self
            .from
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| AlertingError::Config(format!("bad from address '{}': {e}", self.from)))?;

        let mut builder = Message::builder().from(from).subject(subject);
        for to_str in &self.to {
            let to = to_str
                .parse::<lettre::message::Mailbox>()
                .map_err(|e| AlertingError::Config(format!("bad to address '{to_str}': {e}")))?;
            builder = builder.to(to);
        }
        let message = builder
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .map_err(|e| AlertingError::Config(format!("message build: {e}")))?;

        let creds = Credentials::new(self.smtp_user.clone(), self.smtp_password.clone());

        let transport: AsyncSmtpTransport<Tokio1Executor> = match self.tls {
            EmailTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)
                .map_err(|e| AlertingError::Dispatch(format!("smtp relay setup: {e}")))?
                .port(self.smtp_port)
                .credentials(creds)
                .build(),
            EmailTls::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.smtp_host)
                    .map_err(|e| AlertingError::Dispatch(format!("smtp starttls setup: {e}")))?
                    .port(self.smtp_port)
                    .credentials(creds)
                    .build()
            }
            EmailTls::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(self.smtp_host.as_str())
                    .port(self.smtp_port)
                    .credentials(creds)
                    .build()
            }
        };

        transport
            .send(message)
            .await
            .map_err(|e| AlertingError::Dispatch(format!("smtp send: {e}")))?;
        Ok(())
    }
}

/// Build the email subject + plain-text body. Public so tests can
/// assert on the formatting without firing real SMTP.
pub fn email_subject_and_body(p: &AlertNotificationPayload) -> (String, String) {
    let status_tag = match p.status {
        AlertStatus::Firing => "FIRING",
        AlertStatus::Acknowledged => "ACK",
        AlertStatus::Resolved => "RESOLVED",
    };
    let subject = format!(
        "[AppControl][{}][{}] {} — {}",
        status_tag, p.severity, p.policy_name, p.component_name
    );

    let mut body = String::with_capacity(512);
    body.push_str(&format!("Policy:     {}\n", p.policy_name));
    body.push_str(&format!("Application: {}\n", p.app_name));
    body.push_str(&format!("Component:  {}\n", p.component_name));
    body.push_str(&format!("State:      {}\n", p.triggered_state));
    body.push_str(&format!("Severity:   {}\n", p.severity));
    body.push_str(&format!("Status:     {}\n", p.status));
    body.push_str(&format!("Fired at:   {}\n", p.fired_at.to_rfc3339()));
    body.push_str(&format!("Alert ID:   {}\n", p.alert_id));
    if let Some(s) = &p.summary {
        body.push('\n');
        body.push_str(s);
        body.push('\n');
    }
    (subject, body)
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

    // ----- PagerDuty -----

    #[test]
    fn pagerduty_body_maps_firing_to_trigger() {
        let body = pagerduty_body("rk-test", &fixture());
        assert_eq!(body["routing_key"], "rk-test");
        assert_eq!(body["event_action"], "trigger");
        assert_eq!(body["payload"]["severity"], "critical");
        assert_eq!(body["payload"]["source"], "Oracle-DB");
        assert_eq!(body["payload"]["class"], "FAILED");
        assert_eq!(body["payload"]["group"], "Payments");
        assert!(body["dedup_key"].is_string());
    }

    #[test]
    fn pagerduty_body_maps_resolved_to_resolve() {
        let mut p = fixture();
        p.status = AlertStatus::Resolved;
        let body = pagerduty_body("rk", &p);
        assert_eq!(body["event_action"], "resolve");
    }

    #[test]
    fn pagerduty_body_maps_ack_to_acknowledge() {
        let mut p = fixture();
        p.status = AlertStatus::Acknowledged;
        let body = pagerduty_body("rk", &p);
        assert_eq!(body["event_action"], "acknowledge");
    }

    #[test]
    fn pagerduty_dedup_key_uses_alert_id() {
        let p = fixture();
        let body = pagerduty_body("rk", &p);
        assert_eq!(body["dedup_key"].as_str().unwrap(), p.alert_id.to_string());
    }

    #[tokio::test]
    async fn pagerduty_dispatches_to_endpoint() {
        use std::net::SocketAddr;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let captured: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let cap = captured.clone();
        let app = axum::Router::new().route(
            "/enqueue",
            axum::routing::post(
                |axum::Json(body): axum::Json<serde_json::Value>| async move {
                    *cap.lock().await = Some(body);
                    axum::http::StatusCode::ACCEPTED
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let channel = PagerDutyChannel {
            routing_key: "rk-secret".to_string(),
            api_url: format!("http://{addr}/enqueue"),
        };
        let client = build_http_client().unwrap();
        channel.dispatch(&client, &fixture()).await.unwrap();

        let body = captured.lock().await.clone().expect("PD endpoint not hit");
        assert_eq!(body["routing_key"], "rk-secret");
        assert_eq!(body["event_action"], "trigger");
    }

    // ----- MS Teams -----

    #[test]
    fn teams_body_uses_severity_color() {
        let body = teams_body(&fixture());
        assert_eq!(body["@type"], "MessageCard");
        assert_eq!(body["themeColor"], "d93f0b"); // critical
        let title = body["title"].as_str().unwrap();
        assert!(title.contains("Oracle-DB"));
        assert!(title.contains("Database down"));
    }

    #[test]
    fn teams_body_includes_summary_when_set() {
        let body = teams_body(&fixture());
        let facts = body["sections"][0]["facts"].as_array().unwrap();
        assert!(facts.iter().any(|f| f["name"] == "Summary"));
    }

    #[test]
    fn teams_body_omits_summary_when_none() {
        let mut p = fixture();
        p.summary = None;
        let body = teams_body(&p);
        let facts = body["sections"][0]["facts"].as_array().unwrap();
        assert!(facts.iter().all(|f| f["name"] != "Summary"));
    }

    #[tokio::test]
    async fn teams_dispatches_card_to_webhook() {
        use std::net::SocketAddr;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let captured: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let cap = captured.clone();
        let app = axum::Router::new().route(
            "/teams-hook",
            axum::routing::post(
                |axum::Json(body): axum::Json<serde_json::Value>| async move {
                    *cap.lock().await = Some(body);
                    axum::http::StatusCode::OK
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let cfg = NotificationChannelConfig::Teams {
            webhook_url: format!("http://{addr}/teams-hook"),
        };
        let channel = build_channel(&cfg);
        let client = build_http_client().unwrap();
        channel.dispatch(&client, &fixture()).await.unwrap();

        let body = captured
            .lock()
            .await
            .clone()
            .expect("teams endpoint not hit");
        assert_eq!(body["@type"], "MessageCard");
    }

    // ----- Email (subject/body formatting only — SMTP send not tested here) -----

    #[test]
    fn email_subject_includes_severity_and_status() {
        let (subject, _) = email_subject_and_body(&fixture());
        assert!(subject.starts_with("[AppControl][FIRING][critical]"));
        assert!(subject.contains("Database down"));
        assert!(subject.contains("Oracle-DB"));
    }

    #[test]
    fn email_body_contains_metadata_lines() {
        let (_, body) = email_subject_and_body(&fixture());
        assert!(body.contains("Policy:     Database down"));
        assert!(body.contains("Application: Payments"));
        assert!(body.contains("State:      FAILED"));
        assert!(body.contains("Severity:   critical"));
        assert!(body.contains("Status:     firing"));
        assert!(body.contains("Oracle process exited"));
    }

    #[test]
    fn email_subject_resolved_uses_resolved_tag() {
        let mut p = fixture();
        p.status = AlertStatus::Resolved;
        let (subject, _) = email_subject_and_body(&p);
        assert!(subject.contains("[RESOLVED]"));
    }

    #[test]
    fn email_body_omits_summary_block_when_none() {
        let mut p = fixture();
        p.summary = None;
        let (_, body) = email_subject_and_body(&p);
        // Body should not have a trailing blank line + summary text.
        assert!(!body.contains("Oracle process exited"));
    }
}
