use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("HTTP error: {0}")]
    Http(String),
}

/// Circuit breaker state for a webhook endpoint.
/// After `FAILURE_THRESHOLD` consecutive failures, the circuit opens and
/// the endpoint is skipped for `COOLDOWN_SECS` seconds before retrying.
struct CircuitState {
    consecutive_failures: u32,
    last_failure_at: Option<std::time::Instant>,
}

const FAILURE_THRESHOLD: u32 = 5;
const COOLDOWN_SECS: u64 = 300; // 5 minutes

/// Global circuit breaker registry (webhook_id → state).
static CIRCUIT_BREAKERS: std::sync::LazyLock<DashMap<Uuid, CircuitState>> =
    std::sync::LazyLock::new(DashMap::new);

/// Check if a webhook is currently circuit-broken (open).
fn is_circuit_open(webhook_id: Uuid) -> bool {
    if let Some(state) = CIRCUIT_BREAKERS.get(&webhook_id) {
        if state.consecutive_failures >= FAILURE_THRESHOLD {
            if let Some(last) = state.last_failure_at {
                // Allow a retry after cooldown
                if last.elapsed().as_secs() < COOLDOWN_SECS {
                    return true;
                }
            }
        }
    }
    false
}

/// Record a successful delivery (resets the circuit breaker).
fn record_success(webhook_id: Uuid) {
    CIRCUIT_BREAKERS.remove(&webhook_id);
}

/// Record a failed delivery (increments failure counter).
fn record_failure(webhook_id: Uuid) {
    let mut entry = CIRCUIT_BREAKERS.entry(webhook_id).or_insert(CircuitState {
        consecutive_failures: 0,
        last_failure_at: None,
    });
    entry.consecutive_failures += 1;
    entry.last_failure_at = Some(std::time::Instant::now());
}

/// Events that trigger notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationEvent {
    #[serde(rename = "state_change")]
    StateChange {
        component_id: Uuid,
        app_id: Uuid,
        from: String,
        to: String,
    },
    #[serde(rename = "switchover")]
    Switchover {
        app_id: Uuid,
        switchover_id: Uuid,
        phase: String,
        status: String,
    },
    #[serde(rename = "operation")]
    Operation {
        app_id: Uuid,
        operation: String,
        status: String,
        user_id: Uuid,
    },
    #[serde(rename = "failure")]
    Failure {
        component_id: Uuid,
        app_id: Uuid,
        details: String,
    },
}

impl NotificationEvent {
    /// Return the event type string for filtering against webhook subscriptions.
    pub fn event_type(&self) -> &str {
        match self {
            Self::StateChange { .. } => "state_change",
            Self::Switchover { .. } => "switchover",
            Self::Operation { .. } => "operation",
            Self::Failure { .. } => "failure",
        }
    }
}

/// Dispatch a notification event to all matching webhook endpoints.
pub async fn dispatch_event(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    event: NotificationEvent,
) -> Result<(), NotificationError> {
    let event_type = event.event_type().to_string();

    // Find all enabled webhook endpoints that subscribe to this event type,
    // scoped to the application's organization or the specific application.
    #[cfg(feature = "postgres")]
    let webhook_sql: &str = r#"
        SELECT w.id, w.url, w.secret, w.headers
        FROM webhook_endpoints w
        JOIN applications a ON a.organization_id = w.organization_id
        WHERE a.id = $1
          AND w.is_enabled = true
          AND (w.application_id IS NULL OR w.application_id = $1)
          AND w.event_types @> $2::jsonb
        "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let webhook_sql: &str = r#"
        SELECT w.id, w.url, w.secret, w.headers
        FROM webhook_endpoints w
        JOIN applications a ON a.organization_id = w.organization_id
        WHERE a.id = $1
          AND w.is_enabled = 1
          AND (w.application_id IS NULL OR w.application_id = $1)
          AND EXISTS (
              SELECT 1 FROM json_each(w.event_types)
              WHERE json_each.value = $2
          )
        "#;

    #[cfg(feature = "postgres")]
    let event_bind = serde_json::json!([&event_type]);
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let event_bind = serde_json::json!(&event_type);

    let webhooks = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<sqlx::types::Json<serde_json::Value>>,
        ),
    >(webhook_sql)
    .bind(crate::db::bind_id(app_id))
    .bind(event_bind)
    .fetch_all(pool)
    .await
    .map_err(|e| NotificationError::Database(e.to_string()))?;

    if webhooks.is_empty() {
        return Ok(());
    }

    let payload =
        serde_json::to_value(&event).map_err(|e| NotificationError::Http(e.to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| NotificationError::Http(e.to_string()))?;

    for (webhook_id, url, secret, custom_headers) in &webhooks {
        let webhook_id = *webhook_id;

        // Circuit breaker: skip endpoints that are consistently failing
        if is_circuit_open(webhook_id) {
            tracing::debug!(
                webhook_id = %webhook_id,
                "Webhook circuit breaker OPEN — skipping delivery (will retry after cooldown)"
            );
            continue;
        }

        let pool = pool.clone();
        let client = client.clone();
        let url = url.clone();
        let secret = secret.clone();
        let custom_headers = custom_headers.clone();
        let payload = payload.clone();
        let event_type = event_type.clone();

        tokio::spawn(async move {
            deliver_webhook(
                &pool,
                &client,
                webhook_id,
                &url,
                secret.as_deref(),
                custom_headers.as_ref().map(|h| &h.0),
                &event_type,
                &payload,
            )
            .await;
        });
    }

    Ok(())
}

/// Deliver a single webhook with retry logic.
#[allow(clippy::too_many_arguments)]
async fn deliver_webhook(
    pool: &crate::db::DbPool,
    client: &reqwest::Client,
    webhook_id: Uuid,
    url: &str,
    secret: Option<&str>,
    custom_headers: Option<&serde_json::Value>,
    event_type: &str,
    payload: &serde_json::Value,
) {
    let body = serde_json::json!({
        "event": event_type,
        "data": payload,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let max_attempts = 3;
    for attempt in 1..=max_attempts {
        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-AppControl-Event", event_type);

        // Add HMAC signature if a secret is configured
        if let Some(secret) = secret {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            type HmacSha256 = Hmac<Sha256>;
            if let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) {
                let body_str = serde_json::to_string(&body).unwrap_or_default();
                mac.update(body_str.as_bytes());
                let signature = hex::encode(mac.finalize().into_bytes());
                req = req.header("X-AppControl-Signature", format!("sha256={}", signature));
            }
        }

        // Add custom headers
        if let Some(headers) = custom_headers {
            if let Some(obj) = headers.as_object() {
                for (key, val) in obj {
                    if let Some(v) = val.as_str() {
                        if let (Ok(name), Ok(value)) = (
                            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                            reqwest::header::HeaderValue::from_str(v),
                        ) {
                            req = req.header(name, value);
                        }
                    }
                }
            }
        }

        match req.json(&body).send().await {
            Ok(resp) => {
                let status_code = resp.status().as_u16() as i32;
                let response_body = resp.text().await.unwrap_or_default();

                // Record delivery attempt
                let _ = sqlx::query(
                    r#"
                    INSERT INTO webhook_deliveries (webhook_id, event_type, payload, status_code, response_body, attempt)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    "#,
                )
                .bind(webhook_id)
                .bind(event_type)
                .bind(payload)
                .bind(status_code)
                .bind(&response_body)
                .bind(attempt)
                .execute(pool)
                .await;

                // Update last triggered timestamp
                let _ = sqlx::query(
                    &format!("UPDATE webhook_endpoints SET last_triggered_at = {}, last_status_code = $2 WHERE id = $1", crate::db::sql::now()),
                )
                .bind(webhook_id)
                .bind(status_code)
                .execute(pool)
                .await;

                if (200..300).contains(&(status_code as u16 as i32)) {
                    record_success(webhook_id);
                    tracing::debug!(
                        webhook_id = %webhook_id,
                        status = status_code,
                        "Webhook delivered successfully"
                    );
                    return;
                }

                tracing::warn!(
                    webhook_id = %webhook_id,
                    status = status_code,
                    attempt = attempt,
                    "Webhook delivery failed with status {}",
                    status_code
                );
            }
            Err(e) => {
                tracing::warn!(
                    webhook_id = %webhook_id,
                    attempt = attempt,
                    "Webhook delivery error: {}",
                    e
                );

                // Record failed attempt
                let _ = sqlx::query(
                    r#"
                    INSERT INTO webhook_deliveries (webhook_id, event_type, payload, response_body, attempt)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                )
                .bind(webhook_id)
                .bind(event_type)
                .bind(payload)
                .bind(format!("Error: {}", e))
                .bind(attempt)
                .execute(pool)
                .await;
            }
        }

        // Exponential backoff between retries
        if attempt < max_attempts {
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt as u32))).await;
        }
    }

    // All retries exhausted — record failure for circuit breaker
    record_failure(webhook_id);
    tracing::warn!(
        webhook_id = %webhook_id,
        "All webhook delivery attempts exhausted — recording failure for circuit breaker"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_strings() {
        let ev = NotificationEvent::StateChange {
            component_id: Uuid::new_v4(),
            app_id: Uuid::new_v4(),
            from: "STOPPED".to_string(),
            to: "RUNNING".to_string(),
        };
        assert_eq!(ev.event_type(), "state_change");

        let ev = NotificationEvent::Switchover {
            app_id: Uuid::new_v4(),
            switchover_id: Uuid::new_v4(),
            phase: "VALIDATE".to_string(),
            status: "in_progress".to_string(),
        };
        assert_eq!(ev.event_type(), "switchover");

        let ev = NotificationEvent::Operation {
            app_id: Uuid::new_v4(),
            operation: "start".to_string(),
            status: "completed".to_string(),
            user_id: Uuid::new_v4(),
        };
        assert_eq!(ev.event_type(), "operation");

        let ev = NotificationEvent::Failure {
            component_id: Uuid::new_v4(),
            app_id: Uuid::new_v4(),
            details: "process crashed".to_string(),
        };
        assert_eq!(ev.event_type(), "failure");
    }

    #[test]
    fn test_event_serialization() {
        let ev = NotificationEvent::StateChange {
            component_id: Uuid::new_v4(),
            app_id: Uuid::new_v4(),
            from: "STOPPED".to_string(),
            to: "RUNNING".to_string(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "state_change");
        assert_eq!(json["from"], "STOPPED");
        assert_eq!(json["to"], "RUNNING");
    }
}
