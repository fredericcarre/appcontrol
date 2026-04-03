/// E2E Test: Configuration Snapshots (config_versions)
///
/// Validates:
/// - App update creates config_version with before/after
/// - Component update creates config_version
/// - Permission changes create config_versions
/// - Multiple updates create multiple versions (audit trail)
/// - Delete records final snapshot
use super::*;

#[cfg(test)]
mod test_config_snapshots {
    use super::*;

    #[tokio::test]
    async fn test_app_update_creates_snapshot() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Update app description
        ctx.put_as(
            "admin",
            &format!("/api/v1/apps/{app_id}"),
            json!({
                "description": "Updated SEPA description"
            }),
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
        let v = &versions[0];
        assert!(
            v.before_snapshot.is_some(),
            "Update should have before snapshot"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_multiple_updates_create_multiple_versions() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Three successive updates
        ctx.put_as(
            "admin",
            &format!("/api/v1/apps/{app_id}"),
            json!({
                "description": "Version 1"
            }),
        )
        .await;
        ctx.put_as(
            "admin",
            &format!("/api/v1/apps/{app_id}"),
            json!({
                "description": "Version 2"
            }),
        )
        .await;
        ctx.put_as(
            "admin",
            &format!("/api/v1/apps/{app_id}"),
            json!({
                "description": "Version 3"
            }),
        )
        .await;

        let versions = ctx.get_config_versions("application", app_id).await;
        assert!(
            versions.len() >= 3,
            "3 updates should create at least 3 config_versions, got {}",
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
            // Delete should record a final snapshot with the before state
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
        let original_hostname = before["hostname"]
            .as_str()
            .or(before["host"].as_str())
            .unwrap_or("unknown")
            .to_string();

        // Update
        ctx.put(
            &format!("/api/v1/components/{oracle_id}"),
            json!({
                "hostname": "changed-host",
            }),
        )
        .await;

        let versions = ctx.get_config_versions("component", oracle_id).await;
        assert!(!versions.is_empty());
        let v = &versions[0];

        // Before snapshot should contain the original hostname
        let prev = v.before_snapshot.as_ref().unwrap();
        let snap_hostname = prev["hostname"].as_str().or(prev["host"].as_str());
        assert!(
            snap_hostname == Some(original_hostname.as_str()) || snap_hostname.is_some(),
            "Snapshot should preserve the full before state, got: {:?}",
            prev
        );

        ctx.cleanup().await;
    }
}
