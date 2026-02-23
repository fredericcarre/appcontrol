//! E2E tests: Agent and gateway enrollment via tokens.
//!
//! Tests the complete enrollment flow:
//! 1. PKI init (generate CA for org)
//! 2. Token creation / listing / revocation
//! 3. Agent enrollment (unauthenticated, token-based)
//! 4. Token usage limits and expiry enforcement
//! 5. Enrollment audit trail

use super::*;

// ---------------------------------------------------------------------------
// PKI initialization
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pki_init_creates_ca() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/pki/init",
            json!({ "org_name": "Test Corp", "validity_days": 365 }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert_eq!(body["status"], "initialized");
    assert!(body["ca_fingerprint"].as_str().unwrap().len() == 64);
    assert!(body["ca_cert_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN CERTIFICATE"));

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_pki_init_rejects_duplicate() {
    let ctx = TestContext::new().await;

    // First init succeeds
    let resp = ctx
        .post(
            "/api/v1/pki/init",
            json!({ "org_name": "Test Corp" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    // Second init fails (CA already exists)
    let resp = ctx
        .post(
            "/api/v1/pki/init",
            json!({ "org_name": "Test Corp" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 409);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_ca_cert() {
    let ctx = TestContext::new().await;

    // No CA yet
    let resp = ctx.get("/api/v1/pki/ca").await;
    assert_eq!(resp.status().as_u16(), 404);

    // Init CA
    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    // Now it should return the CA
    let resp = ctx.get("/api/v1/pki/ca").await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert!(body["ca_cert_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN CERTIFICATE"));
    assert!(body["fingerprint"].as_str().unwrap().len() == 64);

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Token management
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_enrollment_token() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({
                "name": "deploy-batch-01",
                "max_uses": 10,
                "valid_hours": 48,
                "scope": "agent"
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert!(body["token"].as_str().unwrap().starts_with("ac_enroll_"));
    assert_eq!(body["name"], "deploy-batch-01");
    assert_eq!(body["max_uses"], 10);
    assert_eq!(body["scope"], "agent");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_create_gateway_token() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({
                "name": "gateway-setup",
                "max_uses": 1,
                "scope": "gateway"
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert_eq!(body["scope"], "gateway");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_list_enrollment_tokens() {
    let ctx = TestContext::new().await;

    // Create two tokens
    ctx.post(
        "/api/v1/enrollment/tokens",
        json!({ "name": "token-1", "scope": "agent" }),
    )
    .await;
    ctx.post(
        "/api/v1/enrollment/tokens",
        json!({ "name": "token-2", "scope": "gateway" }),
    )
    .await;

    let resp = ctx.get("/api/v1/enrollment/tokens").await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    let tokens = body["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_revoke_enrollment_token() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "to-revoke" }),
        )
        .await;
    let body: Value = resp.json().await;
    let token_id = body["id"].as_str().unwrap();

    // Revoke
    let resp = ctx
        .post(
            &format!("/api/v1/enrollment/tokens/{}/revoke", token_id),
            json!({}),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    // Check it shows as revoked
    let resp = ctx.get("/api/v1/enrollment/tokens").await;
    let body: Value = resp.json().await;
    let tokens = body["tokens"].as_array().unwrap();
    assert!(tokens[0]["revoked_at"].as_str().is_some());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_invalid_scope_rejected() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "bad", "scope": "admin" }),
        )
        .await;
    // Should fail validation
    assert!(resp.status().as_u16() >= 400);

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Agent enrollment (unauthenticated, token-based)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_enrollment_success() {
    let ctx = TestContext::new().await;

    // Init PKI
    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    // Create token
    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "agent-deploy", "max_uses": 5 }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // Enroll agent (unauthenticated call)
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({
                "token": token,
                "hostname": "server01.prod"
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert!(body["cert_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN CERTIFICATE"));
    assert!(body["key_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN PRIVATE KEY"));
    assert!(body["ca_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN CERTIFICATE"));
    assert!(body["fingerprint"].as_str().unwrap().len() == 64);
    assert!(body["agent_id"].as_str().is_some());
    assert_eq!(body["expires_in_days"], 365);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_enrollment_increments_usage() {
    let ctx = TestContext::new().await;

    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "limited", "max_uses": 2 }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // First enrollment
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "host1.prod" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    // Second enrollment
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "host2.prod" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    // Third enrollment should fail (max_uses=2)
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "host3.prod" }),
        )
        .await;
    assert!(resp.status().as_u16() >= 400);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_enrollment_with_revoked_token_fails() {
    let ctx = TestContext::new().await;

    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "to-revoke" }),
        )
        .await;
    let body: Value = resp.json().await;
    let token_id = body["id"].as_str().unwrap().to_string();
    let token = body["token"].as_str().unwrap().to_string();

    // Revoke the token
    ctx.post(
        &format!("/api/v1/enrollment/tokens/{}/revoke", token_id),
        json!({}),
    )
    .await;

    // Try to enroll with revoked token
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "server01.prod" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 401);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_enrollment_with_invalid_token_fails() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({
                "token": "ac_enroll_0000000000000000000000000000dead",
                "hostname": "server01.prod"
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 401);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_enrollment_without_pki_init_fails() {
    let ctx = TestContext::new().await;

    // Create token (no PKI init)
    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "no-ca" }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // Enrollment should fail because CA is not initialized
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "server01" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 500);

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Gateway enrollment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gateway_enrollment() {
    let ctx = TestContext::new().await;

    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "gw-token", "scope": "gateway", "max_uses": 1 }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // Enroll gateway with SANs
    let resp = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({
                "token": &token,
                "hostname": "gateway.prod.example.com",
                "san_dns": ["gw.prod.example.com", "localhost"],
                "san_ips": ["10.0.1.5", "127.0.0.1"],
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    assert!(body["cert_pem"]
        .as_str()
        .unwrap()
        .contains("BEGIN CERTIFICATE"));

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Enrollment audit trail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_enrollment_events_logged() {
    let ctx = TestContext::new().await;

    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "audited" }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // Successful enrollment
    ctx.post_anonymous(
        "/api/v1/enroll",
        json!({ "token": &token, "hostname": "host1.prod" }),
    )
    .await;

    // Failed enrollment (bad token)
    ctx.post_anonymous(
        "/api/v1/enroll",
        json!({ "token": "ac_enroll_00000000000000000000000000000bad", "hostname": "hacker" }),
    )
    .await;

    // Check audit trail
    let resp = ctx.get("/api/v1/enrollment/events").await;
    assert_eq!(resp.status().as_u16(), 200);

    let body: Value = resp.json().await;
    let events = body["events"].as_array().unwrap();
    assert!(events.len() >= 1);

    // Find the success event
    let success_event = events
        .iter()
        .find(|e| e["event_type"] == "success")
        .unwrap();
    assert_eq!(success_event["hostname"], "host1.prod");
    assert!(success_event["cert_fingerprint"].as_str().is_some());

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Multiple agents with same token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_agents_different_certs() {
    let ctx = TestContext::new().await;

    ctx.post(
        "/api/v1/pki/init",
        json!({ "org_name": "Test Corp" }),
    )
    .await;

    let resp = ctx
        .post(
            "/api/v1/enrollment/tokens",
            json!({ "name": "batch-deploy", "max_uses": 100 }),
        )
        .await;
    let body: Value = resp.json().await;
    let token = body["token"].as_str().unwrap().to_string();

    // Enroll two different agents
    let resp1 = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "web01.prod" }),
        )
        .await;
    let body1: Value = resp1.json().await;

    let resp2 = ctx
        .post_anonymous(
            "/api/v1/enroll",
            json!({ "token": &token, "hostname": "web02.prod" }),
        )
        .await;
    let body2: Value = resp2.json().await;

    // Different certs, different fingerprints
    assert_ne!(body1["cert_pem"], body2["cert_pem"]);
    assert_ne!(body1["fingerprint"], body2["fingerprint"]);
    assert_ne!(body1["agent_id"], body2["agent_id"]);

    // Same CA
    assert_eq!(body1["ca_pem"], body2["ca_pem"]);

    ctx.cleanup().await;
}
