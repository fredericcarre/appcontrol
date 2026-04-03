//! Certificate rotation manager for seamless CA migration.
//!
//! This module handles the lifecycle of CA certificate rotation:
//! 1. Start rotation: Import new CA, notify all connected entities
//! 2. Track progress: Monitor which agents/gateways have migrated
//! 3. Finalize: Once all entities migrated, swap to new CA
//!
//! During rotation, both old and new CAs are trusted (dual-trust period).
//! This allows for a gradual, zero-downtime migration.

use crate::db::DbPool;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    pool: &DbPool,
    org_id: Uuid,
    new_ca_cert_pem: &str,
    new_ca_key_pem: &str,
    grace_period_secs: u64,
    initiated_by: Uuid,
) -> Result<Uuid, ApiError> {
    // Validate the new CA keypair
    appcontrol_common::validate_ca_keypair(new_ca_cert_pem, new_ca_key_pem)
        .map_err(|e| ApiError::Validation(format!("Invalid CA keypair: {}", e)))?;

    use crate::repository::core_queries as repo;

    // Check if there's already a rotation in progress
    let existing = repo::find_active_rotation(pool, org_id).await?;
    if existing.is_some() {
        return Err(ApiError::Conflict("A certificate rotation is already in progress".to_string()));
    }

    // Check that the organization has an existing CA
    let current_ca = repo::get_current_ca(pool, org_id).await?;
    if current_ca.is_none() || current_ca.as_ref().and_then(|c| c.0.as_ref()).is_none() {
        return Err(ApiError::Validation("No existing CA to rotate from. Use PKI init instead.".to_string()));
    }

    let rotation_id = Uuid::new_v4();

    // Count total agents and gateways that need to migrate
    let agent_count = repo::count_certified_agents(pool, org_id).await?;
    let gateway_count = repo::count_certified_gateways(pool, org_id).await?;

    // Start transaction to update org and create progress record
    repo::start_rotation_tx(
        pool, org_id, rotation_id, new_ca_cert_pem, new_ca_key_pem,
        agent_count.0, gateway_count.0, initiated_by, grace_period_secs as i32,
    ).await?;

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
#[allow(clippy::too_many_arguments)]
pub async fn record_migration(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fingerprint: &str,
    new_fingerprint: &str,
    hostname: &str,
) -> Result<(), ApiError> {
    use crate::repository::core_queries as repo;

    repo::insert_cert_migration(pool, org_id, rotation_id, agent_id, gateway_id, old_fingerprint, new_fingerprint, hostname).await?;

    if agent_id.is_some() {
        repo::increment_migrated_agents(pool, org_id, rotation_id).await?;
    } else if gateway_id.is_some() {
        repo::increment_migrated_gateways(pool, org_id, rotation_id).await?;
    }

    // Check if rotation is complete
    check_rotation_completion(pool, org_id, rotation_id).await?;

    Ok(())
}

/// Record that an entity failed to rotate.
#[allow(clippy::too_many_arguments)]
pub async fn record_migration_failure(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fingerprint: &str,
    hostname: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    use crate::repository::core_queries as repo;

    repo::insert_cert_migration_failure(pool, org_id, rotation_id, agent_id, gateway_id, old_fingerprint, hostname, error_message).await?;

    if agent_id.is_some() {
        repo::increment_failed_agents(pool, org_id, rotation_id).await?;
    } else if gateway_id.is_some() {
        repo::increment_failed_gateways(pool, org_id, rotation_id).await?;
    }

    Ok(())
}

/// Check if all entities have migrated and update status.
async fn check_rotation_completion(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<bool, ApiError> {
    use crate::repository::core_queries as repo;

    let progress = repo::get_rotation_counts(pool, org_id, rotation_id).await?;

    if let Some((total_agents, total_gateways, migrated_agents, migrated_gateways)) = progress {
        if migrated_agents >= total_agents && migrated_gateways >= total_gateways {
            repo::mark_rotation_ready(pool, org_id, rotation_id).await?;

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
#[allow(clippy::type_complexity)]
pub async fn get_rotation_progress(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<RotationProgress>, ApiError> {
    use crate::repository::core_queries as repo;

    let row = repo::get_rotation_progress_details(pool, org_id).await?;

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
        let ca_row = repo::get_ca_certs(pool, org_id).await?;

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
pub async fn finalize_rotation(pool: &DbPool, org_id: Uuid) -> Result<(), ApiError> {
    use crate::repository::core_queries as repo;

    let progress = repo::get_rotation_status(pool, org_id).await?;
    let rotation_id = match progress {
        Some((rid, status)) if status == "ready" => rid,
        Some((_, status)) => {
            return Err(ApiError::Validation(format!("Cannot finalize rotation in status '{}'. Must be 'ready'.", status)));
        }
        None => { return Err(ApiError::NotFound); }
    };

    // Start transaction to swap CAs
    repo::finalize_rotation_tx(pool, org_id, rotation_id).await?;

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
pub async fn cancel_rotation(pool: &DbPool, org_id: Uuid) -> Result<(), ApiError> {
    use crate::repository::core_queries as repo;

    let progress = repo::get_rotation_status(pool, org_id).await?;
    let rotation_id = match progress {
        Some((rid, status)) if status == "in_progress" => rid,
        Some((_, status)) => { return Err(ApiError::Validation(format!("Cannot cancel rotation in status '{}'", status))); }
        None => { return Err(ApiError::NotFound); }
    };

    use crate::repository::core_queries as repo_core;
    repo_core::cancel_rotation_tx(pool, org_id, rotation_id).await?;

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
pub async fn get_ca_bundle(pool: &DbPool, org_id: Uuid) -> Result<String, ApiError> {
    let row = crate::repository::core_queries::get_ca_certs(pool, org_id).await?;

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
