//! Certificate rotation manager for seamless CA migration.
//!
//! This module handles the lifecycle of CA certificate rotation:
//! 1. Start rotation: Import new CA, notify all connected entities
//! 2. Track progress: Monitor which agents/gateways have migrated
//! 3. Finalize: Once all entities migrated, swap to new CA
//!
//! During rotation, both old and new CAs are trusted (dual-trust period).
//! This allows for a gradual, zero-downtime migration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;

/// Status of a certificate rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RotationStatus {
    /// Rotation is in progress, entities are migrating
    InProgress,
    /// All entities have migrated, ready to finalize
    Ready,
    /// Rotation completed and finalized
    Completed,
    /// Rotation was cancelled
    Cancelled,
    /// Rotation failed
    Failed,
}

impl std::fmt::Display for RotationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RotationStatus::InProgress => write!(f, "in_progress"),
            RotationStatus::Ready => write!(f, "ready"),
            RotationStatus::Completed => write!(f, "completed"),
            RotationStatus::Cancelled => write!(f, "cancelled"),
            RotationStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Progress of a certificate rotation.
#[derive(Debug, Clone, Serialize)]
pub struct RotationProgress {
    pub rotation_id: Uuid,
    pub organization_id: Uuid,
    pub status: String,
    pub total_agents: i32,
    pub total_gateways: i32,
    pub migrated_agents: i32,
    pub migrated_gateways: i32,
    pub failed_agents: i32,
    pub failed_gateways: i32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub grace_period_secs: i32,
    pub old_ca_fingerprint: Option<String>,
    pub new_ca_fingerprint: Option<String>,
}

/// Start a new certificate rotation.
///
/// This imports the new CA and initiates the rotation process.
/// Connected gateways will be notified to forward the rotation command to agents.
pub async fn start_rotation(
    pool: &PgPool,
    org_id: Uuid,
    new_ca_cert_pem: &str,
    new_ca_key_pem: &str,
    grace_period_secs: u64,
    initiated_by: Uuid,
) -> Result<Uuid, ApiError> {
    // Validate the new CA keypair
    appcontrol_common::validate_ca_keypair(new_ca_cert_pem, new_ca_key_pem)
        .map_err(|e| ApiError::Validation(format!("Invalid CA keypair: {}", e)))?;

    // Check if there's already a rotation in progress
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT rotation_id FROM rotation_progress
           WHERE organization_id = $1 AND status = 'in_progress'"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    if existing.is_some() {
        return Err(ApiError::Conflict(
            "A certificate rotation is already in progress".to_string(),
        ));
    }

    // Check that the organization has an existing CA
    let current_ca: Option<(Option<String>,)> =
        sqlx::query_as("SELECT ca_cert_pem FROM organizations WHERE id = $1")
            .bind(org_id)
            .fetch_optional(pool)
            .await?;

    if current_ca.is_none() || current_ca.as_ref().and_then(|c| c.0.as_ref()).is_none() {
        return Err(ApiError::Validation(
            "No existing CA to rotate from. Use PKI init instead.".to_string(),
        ));
    }

    let rotation_id = Uuid::new_v4();

    // Count total agents and gateways that need to migrate
    let agent_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM agents WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;

    let gateway_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM gateways WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;

    // Start transaction to update org and create progress record
    let mut tx = pool.begin().await?;

    // Store pending CA
    sqlx::query(
        r#"UPDATE organizations
           SET pending_ca_cert_pem = $2, pending_ca_key_pem = $3, rotation_started_at = now()
           WHERE id = $1"#,
    )
    .bind(org_id)
    .bind(new_ca_cert_pem)
    .bind(new_ca_key_pem)
    .execute(&mut *tx)
    .await?;

    // Create progress tracking record
    sqlx::query(
        r#"INSERT INTO rotation_progress
           (organization_id, rotation_id, total_agents, total_gateways, initiated_by, grace_period_secs)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .bind(agent_count.0 as i32)
    .bind(gateway_count.0 as i32)
    .bind(initiated_by)
    .bind(grace_period_secs as i32)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let new_fp = appcontrol_common::fingerprint_pem(new_ca_cert_pem).unwrap_or_default();
    tracing::info!(
        org_id = %org_id,
        rotation_id = %rotation_id,
        new_ca_fingerprint = %new_fp,
        total_agents = agent_count.0,
        total_gateways = gateway_count.0,
        "Certificate rotation started"
    );

    Ok(rotation_id)
}

/// Record that an entity has successfully rotated to the new CA.
pub async fn record_migration(
    pool: &PgPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fingerprint: &str,
    new_fingerprint: &str,
    hostname: &str,
) -> Result<(), ApiError> {
    // Insert migration record
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, new_fingerprint, status, hostname)
           VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .bind(agent_id)
    .bind(gateway_id)
    .bind(old_fingerprint)
    .bind(new_fingerprint)
    .bind(hostname)
    .execute(pool)
    .await?;

    // Update progress counter
    if agent_id.is_some() {
        sqlx::query(
            r#"UPDATE rotation_progress
               SET migrated_agents = migrated_agents + 1
               WHERE organization_id = $1 AND rotation_id = $2"#,
        )
        .bind(org_id)
        .bind(rotation_id)
        .execute(pool)
        .await?;
    } else if gateway_id.is_some() {
        sqlx::query(
            r#"UPDATE rotation_progress
               SET migrated_gateways = migrated_gateways + 1
               WHERE organization_id = $1 AND rotation_id = $2"#,
        )
        .bind(org_id)
        .bind(rotation_id)
        .execute(pool)
        .await?;
    }

    // Check if rotation is complete
    check_rotation_completion(pool, org_id, rotation_id).await?;

    Ok(())
}

/// Record that an entity failed to rotate.
pub async fn record_migration_failure(
    pool: &PgPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fingerprint: &str,
    hostname: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, status, hostname, error_message)
           VALUES ($1, $2, $3, $4, $5, 'failed', $6, $7)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .bind(agent_id)
    .bind(gateway_id)
    .bind(old_fingerprint)
    .bind(hostname)
    .bind(error_message)
    .execute(pool)
    .await?;

    // Update failure counter
    if agent_id.is_some() {
        sqlx::query(
            r#"UPDATE rotation_progress
               SET failed_agents = failed_agents + 1
               WHERE organization_id = $1 AND rotation_id = $2"#,
        )
        .bind(org_id)
        .bind(rotation_id)
        .execute(pool)
        .await?;
    } else if gateway_id.is_some() {
        sqlx::query(
            r#"UPDATE rotation_progress
               SET failed_gateways = failed_gateways + 1
               WHERE organization_id = $1 AND rotation_id = $2"#,
        )
        .bind(org_id)
        .bind(rotation_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Check if all entities have migrated and update status.
async fn check_rotation_completion(
    pool: &PgPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<bool, ApiError> {
    let progress: Option<(i32, i32, i32, i32)> = sqlx::query_as(
        r#"SELECT total_agents, total_gateways, migrated_agents, migrated_gateways
           FROM rotation_progress
           WHERE organization_id = $1 AND rotation_id = $2"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .fetch_optional(pool)
    .await?;

    if let Some((total_agents, total_gateways, migrated_agents, migrated_gateways)) = progress {
        if migrated_agents >= total_agents && migrated_gateways >= total_gateways {
            sqlx::query(
                r#"UPDATE rotation_progress
                   SET status = 'ready', completed_at = now()
                   WHERE organization_id = $1 AND rotation_id = $2 AND status = 'in_progress'"#,
            )
            .bind(org_id)
            .bind(rotation_id)
            .execute(pool)
            .await?;

            tracing::info!(
                org_id = %org_id,
                rotation_id = %rotation_id,
                "Certificate rotation ready for finalization"
            );

            return Ok(true);
        }
    }

    Ok(false)
}

/// Get the current rotation progress.
pub async fn get_rotation_progress(
    pool: &PgPool,
    org_id: Uuid,
) -> Result<Option<RotationProgress>, ApiError> {
    let row: Option<(
        Uuid,
        String,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
        i32,
    )> = sqlx::query_as(
        r#"SELECT rotation_id, status, total_agents, total_gateways,
                  migrated_agents, migrated_gateways, failed_agents, failed_gateways,
                  started_at, completed_at, finalized_at, grace_period_secs
           FROM rotation_progress
           WHERE organization_id = $1
           ORDER BY started_at DESC
           LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    if let Some((
        rotation_id,
        status,
        total_agents,
        total_gateways,
        migrated_agents,
        migrated_gateways,
        failed_agents,
        failed_gateways,
        started_at,
        completed_at,
        finalized_at,
        grace_period_secs,
    )) = row
    {
        // Get fingerprints
        let ca_row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT ca_cert_pem, pending_ca_cert_pem FROM organizations WHERE id = $1",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await?;

        let (old_fp, new_fp) = if let Some((current_ca, pending_ca)) = ca_row {
            (
                current_ca.and_then(|c| appcontrol_common::fingerprint_pem(&c)),
                pending_ca.and_then(|c| appcontrol_common::fingerprint_pem(&c)),
            )
        } else {
            (None, None)
        };

        return Ok(Some(RotationProgress {
            rotation_id,
            organization_id: org_id,
            status,
            total_agents,
            total_gateways,
            migrated_agents,
            migrated_gateways,
            failed_agents,
            failed_gateways,
            started_at,
            completed_at,
            finalized_at,
            grace_period_secs,
            old_ca_fingerprint: old_fp,
            new_ca_fingerprint: new_fp,
        }));
    }

    Ok(None)
}

/// Finalize the rotation by swapping the pending CA to primary.
///
/// This should only be called after all entities have migrated (status = 'ready').
/// After finalization, the old CA is removed and only the new CA is trusted.
pub async fn finalize_rotation(pool: &PgPool, org_id: Uuid) -> Result<(), ApiError> {
    // Check that rotation is ready
    let progress: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT rotation_id, status FROM rotation_progress
           WHERE organization_id = $1
           ORDER BY started_at DESC
           LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let rotation_id = match progress {
        Some((rid, status)) if status == "ready" => rid,
        Some((_, status)) => {
            return Err(ApiError::Validation(format!(
                "Cannot finalize rotation in status '{}'. Must be 'ready'.",
                status
            )));
        }
        None => {
            return Err(ApiError::NotFound);
        }
    };

    // Start transaction to swap CAs
    let mut tx = pool.begin().await?;

    // Move pending CA to primary, clear pending
    sqlx::query(
        r#"UPDATE organizations
           SET ca_cert_pem = pending_ca_cert_pem,
               ca_key_pem = pending_ca_key_pem,
               pending_ca_cert_pem = NULL,
               pending_ca_key_pem = NULL,
               rotation_started_at = NULL
           WHERE id = $1"#,
    )
    .bind(org_id)
    .execute(&mut *tx)
    .await?;

    // Mark rotation as finalized
    sqlx::query(
        r#"UPDATE rotation_progress
           SET status = 'completed', finalized_at = now()
           WHERE organization_id = $1 AND rotation_id = $2"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        org_id = %org_id,
        rotation_id = %rotation_id,
        "Certificate rotation finalized"
    );

    Ok(())
}

/// Cancel an in-progress rotation.
///
/// This clears the pending CA and marks the rotation as cancelled.
/// Entities that already migrated will need to re-enroll with the original CA.
pub async fn cancel_rotation(pool: &PgPool, org_id: Uuid) -> Result<(), ApiError> {
    let progress: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT rotation_id, status FROM rotation_progress
           WHERE organization_id = $1
           ORDER BY started_at DESC
           LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let rotation_id = match progress {
        Some((rid, status)) if status == "in_progress" => rid,
        Some((_, status)) => {
            return Err(ApiError::Validation(format!(
                "Cannot cancel rotation in status '{}'",
                status
            )));
        }
        None => {
            return Err(ApiError::NotFound);
        }
    };

    let mut tx = pool.begin().await?;

    // Clear pending CA
    sqlx::query(
        r#"UPDATE organizations
           SET pending_ca_cert_pem = NULL,
               pending_ca_key_pem = NULL,
               rotation_started_at = NULL
           WHERE id = $1"#,
    )
    .bind(org_id)
    .execute(&mut *tx)
    .await?;

    // Mark rotation as cancelled
    sqlx::query(
        r#"UPDATE rotation_progress
           SET status = 'cancelled'
           WHERE organization_id = $1 AND rotation_id = $2"#,
    )
    .bind(org_id)
    .bind(rotation_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        org_id = %org_id,
        rotation_id = %rotation_id,
        "Certificate rotation cancelled"
    );

    Ok(())
}

/// Get the CA bundle for an organization.
///
/// During rotation, returns both old and new CAs concatenated.
/// Otherwise, returns just the current CA.
pub async fn get_ca_bundle(pool: &PgPool, org_id: Uuid) -> Result<String, ApiError> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ca_cert_pem, pending_ca_cert_pem FROM organizations WHERE id = $1",
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((Some(current), pending)) => {
            if let Some(pending_ca) = pending {
                // During rotation: bundle both CAs
                Ok(format!("{}\n{}", current, pending_ca))
            } else {
                // Normal: just current CA
                Ok(current)
            }
        }
        _ => Err(ApiError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation_status_display() {
        assert_eq!(RotationStatus::InProgress.to_string(), "in_progress");
        assert_eq!(RotationStatus::Ready.to_string(), "ready");
        assert_eq!(RotationStatus::Completed.to_string(), "completed");
        assert_eq!(RotationStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(RotationStatus::Failed.to_string(), "failed");
    }
}
