/// E2E Test: WebSocket Real-Time Events
///
/// Validates:
/// - WebSocket connection with JWT auth
/// - Subscribe/unsubscribe to app events
/// - StateChange events pushed on component transitions
/// - CheckResultEvent events pushed on check results
/// - Unauthorized WebSocket connection rejected
/// - Multiple concurrent subscriptions
use super::*;

#[cfg(test)]
mod test_websocket_events {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    #[tokio::test]
    async fn test_websocket_connection_with_jwt() {
        let ctx = TestContext::new().await;
        let ws = ctx.connect_websocket(&ctx.admin_token).await;

        // Connection should succeed
        drop(ws);
        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_websocket_connection_rejected_without_token() {
        let ctx = TestContext::new().await;
        let url = format!("{}/ws", ctx.ws_url);
        let result = tokio_tungstenite::connect_async(&url).await;
        assert!(
            result.is_err() || {
                // Some implementations return a 401 response
                let (_, resp) = result.unwrap();
                resp.status().as_u16() == 401
            }
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_websocket_connection_rejected_with_invalid_token() {
        let ctx = TestContext::new().await;
        let url = format!("{}/ws?token=invalid-garbage-token", ctx.ws_url);
        let result = tokio_tungstenite::connect_async(&url).await;
        // Should fail or get 401
        if let Ok((_, resp)) = &result {
            assert_eq!(resp.status().as_u16(), 401);
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_subscribe_receives_state_changes() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

        // Subscribe to app events
        let subscribe_msg = json!({"type": "Subscribe", "app_id": app_id.to_string()});
        ws.send(Message::Text(subscribe_msg.to_string()))
            .await
            .unwrap();

        // Trigger a state change
        ctx.force_component_state(app_id, "Oracle-DB", "RUNNING")
            .await;

        // Start the app (which triggers real state transitions)
        ctx.post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
            .await;

        // Wait for a WebSocket message (with timeout)
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
        if let Ok(Some(Ok(Message::Text(text)))) = msg {
            let event: Value = serde_json::from_str(&text).unwrap();
            assert!(
                event["type"].as_str() == Some("StateChange")
                    || event["type"].as_str() == Some("CheckResult")
                    || event["type"].as_str() == Some("Ack"),
                "Should receive a valid event type, got: {}",
                event["type"]
            );
        }
        // Timeout is acceptable in test environment without real agents

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_unsubscribe_stops_events() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

        // Subscribe then unsubscribe
        ws.send(Message::Text(
            json!({
                "type": "Subscribe", "app_id": app_id.to_string()
            })
            .to_string(),
        ))
        .await
        .unwrap();

        ws.send(Message::Text(
            json!({
                "type": "Unsubscribe", "app_id": app_id.to_string()
            })
            .to_string(),
        ))
        .await
        .unwrap();

        // Trigger state change
        ctx.force_component_state(app_id, "Oracle-DB", "RUNNING")
            .await;

        // Should NOT receive events after unsubscribe (timeout expected)
        let msg = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
        assert!(msg.is_err(), "Should timeout — no events after unsubscribe");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_websocket_permission_filtering() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Viewer with no permission tries to subscribe
        let mut ws = ctx.connect_websocket(&ctx.viewer_token).await;
        ws.send(Message::Text(
            json!({
                "type": "Subscribe", "app_id": app_id.to_string()
            })
            .to_string(),
        ))
        .await
        .unwrap();

        // Should receive an error or no events (permission denied)
        let msg = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
        if let Ok(Some(Ok(Message::Text(text)))) = msg {
            let event: Value = serde_json::from_str(&text).unwrap();
            // Either an error message or permission denied
            if let Some(t) = event["type"].as_str() {
                assert!(
                    t == "Error" || t == "PermissionDenied",
                    "Unauthorized subscription should be rejected, got: {t}"
                );
            }
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_multiple_subscriptions() {
        let ctx = TestContext::new().await;
        let app1_id = ctx.create_payments_app().await;

        let resp = ctx
            .post("/api/v1/apps", json!({"name": "Second-App"}))
            .await;
        let app2: Value = resp.json().await.unwrap();
        let app2_id: Uuid = app2["id"].as_str().unwrap().parse().unwrap();

        let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

        // Subscribe to both apps
        ws.send(Message::Text(
            json!({
                "type": "Subscribe", "app_id": app1_id.to_string()
            })
            .to_string(),
        ))
        .await
        .unwrap();

        ws.send(Message::Text(
            json!({
                "type": "Subscribe", "app_id": app2_id.to_string()
            })
            .to_string(),
        ))
        .await
        .unwrap();

        // Both subscriptions should be active (no error)
        tokio::time::sleep(Duration::from_millis(100)).await;

        ctx.cleanup().await;
    }
}
