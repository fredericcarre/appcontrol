//! SQLite E2E: WebSocket real-time events.

use super::common::TestContext;
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::test]
async fn test_websocket_connection_with_jwt() {
    let ctx = TestContext::new().await;
    let ws = ctx.connect_websocket(&ctx.admin_token).await;
    drop(ws);
}

#[tokio::test]
async fn test_websocket_connection_rejected_without_token() {
    let ctx = TestContext::new().await;
    let url = format!("{}/ws", ctx.ws_url);
    let result = tokio_tungstenite::connect_async(&url).await;
    if let Ok((_, resp)) = &result {
        assert_eq!(resp.status().as_u16(), 401);
    }
}

#[tokio::test]
async fn test_websocket_connection_rejected_with_invalid_token() {
    let ctx = TestContext::new().await;
    let url = format!("{}/ws?token=invalid-garbage-token", ctx.ws_url);
    let result = tokio_tungstenite::connect_async(&url).await;
    if let Ok((_, resp)) = &result {
        assert_eq!(resp.status().as_u16(), 401);
    }
}

#[tokio::test]
async fn test_subscribe_to_app_events() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

    let subscribe_msg = json!({"type": "Subscribe", "app_id": app_id.to_string()});
    ws.send(Message::Text(subscribe_msg.to_string()))
        .await
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(3), ws.next()).await;
    if let Ok(Some(Ok(Message::Text(text)))) = msg {
        let event: Value = serde_json::from_str(&text).unwrap();
        let etype = event["type"].as_str().unwrap_or("");
        assert!(
            etype == "Ack" || etype == "StateChange" || etype == "CheckResult",
            "Should receive a valid event type, got: {etype}"
        );
    }
}

#[tokio::test]
async fn test_unsubscribe_stops_events() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

    ws.send(Message::Text(
        json!({"type": "Subscribe", "app_id": app_id.to_string()}).to_string(),
    ))
    .await
    .unwrap();

    ws.send(Message::Text(
        json!({"type": "Unsubscribe", "app_id": app_id.to_string()}).to_string(),
    ))
    .await
    .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
    if let Ok(Some(Ok(Message::Text(text)))) = &msg {
        let event: Value = serde_json::from_str(text).unwrap_or_default();
        let etype = event["type"].as_str().unwrap_or("");
        assert!(
            etype == "Ack" || etype == "Error",
            "After unsubscribe, should only get Ack, got: {etype}"
        );
    }
}

#[tokio::test]
async fn test_multiple_subscriptions() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let ctx = TestContext::new().await;
    let app1_id = ctx.create_payments_app().await;

    let resp = ctx
        .post("/api/v1/apps", json!({"name": "Second-App", "site_id": ctx.default_site_id}))
        .await;
    let app2: Value = resp.json().await.unwrap();
    let app2_id = TestContext::extract_id(&app2);

    let mut ws = ctx.connect_websocket(&ctx.admin_token).await;

    ws.send(Message::Text(
        json!({"type": "Subscribe", "app_id": app1_id.to_string()}).to_string(),
    ))
    .await
    .unwrap();

    ws.send(Message::Text(
        json!({"type": "Subscribe", "app_id": app2_id.to_string()}).to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
}
