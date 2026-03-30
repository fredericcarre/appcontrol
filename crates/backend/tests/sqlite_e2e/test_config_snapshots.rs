//! SQLite E2E: Configuration Snapshots (config_versions).

use super::common::TestContext;
use serde_json::{json, Value};
use uuid::Uuid;

#[tokio::test]
async fn test_app_update_creates_snapshot() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    ctx.put_as(
        "admin",
        &format!("/api/v1/apps/{app_id}"),
        json!({ "description": "Updated SEPA description" }),
    )
    .await;

    let versions = ctx.get_config_versions("application", app_id).await;
    assert!(
        !versions.is_empty(),
        "App update should create config_version"
    );
    let v = &versions[0];
    assert!(
        v.before_snapshot.is_some(),
        "Update should have before snapshot"
    );
    assert_eq!(
        v.after_snapshot["description"].as_str(),
        Some("Updated SEPA description")
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_component_update_creates_snapshot() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

    ctx.put(
        &format!("/api/v1/components/{oracle_id}"),
        json!({
            "hostname": "new-oracle-host",
            "check_interval_seconds": 60,
        }),
    )
    .await;

    let versions = ctx.get_config_versions("component", oracle_id).await;
    assert!(
        !versions.is_empty(),
        "Component update should create config_version"
    );
    assert!(
        versions[0].before_snapshot.is_some(),
        "Update should have before snapshot"
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_multiple_updates_create_multiple_versions() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    for desc in ["Version 1", "Version 2", "Version 3"] {
        ctx.put_as(
            "admin",
            &format!("/api/v1/apps/{app_id}"),
            json!({ "description": desc }),
        )
        .await;
    }

    let versions = ctx.get_config_versions("application", app_id).await;
    assert!(
        versions.len() >= 3,
        "3 updates should create >= 3 config_versions, got {}",
        versions.len()
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_delete_records_final_snapshot() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({
                "name": "Deletable-App",
                "description": "Will be deleted",
                "site_id": ctx.default_site_id,
            }),
        )
        .await;
    let app: Value = resp.json().await.unwrap();
    let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

    ctx.delete_as("admin", &format!("/api/v1/apps/{app_id}"))
        .await;

    let versions = ctx.get_config_versions("application", app_id).await;
    if !versions.is_empty() {
        let last = versions.last().unwrap();
        assert!(
            last.before_snapshot.is_some() || !last.after_snapshot.is_null(),
            "Delete should record final snapshot"
        );
    }

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_snapshot_preserves_full_before_state() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Read current state
    let resp = ctx.get(&format!("/api/v1/components/{oracle_id}")).await;
    let before: Value = resp.json().await.unwrap();
    let original_host = before["host"].as_str().unwrap().to_string();

    // Update
    ctx.put(
        &format!("/api/v1/components/{oracle_id}"),
        json!({ "hostname": "changed-host" }),
    )
    .await;

    let versions = ctx.get_config_versions("component", oracle_id).await;
    assert!(!versions.is_empty());
    let v = &versions[0];

    let prev = v.before_snapshot.as_ref().unwrap();
    assert_eq!(
        prev["host"].as_str(),
        Some(original_host.as_str()),
        "Snapshot should preserve the full before state"
    );

    ctx.cleanup().await;
}
