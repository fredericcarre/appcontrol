//! PKI export endpoints for TLS certificate management.
//!
//! These endpoints support:
//! 1. Unauthenticated CA public cert retrieval (for init containers)
//! 2. Certificate export to shared volumes (for nginx TLS termination)
//! 3. Server certificate issuance (for nginx/gateway TLS)

use axum::{extract::State, response::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Public CA endpoint (unauthenticated - for init containers)
// ---------------------------------------------------------------------------

/// Get the CA public certificate for trust establishment.
///
/// This endpoint is intentionally unauthenticated to allow init containers
/// and agents to retrieve the CA certificate before they have credentials.
/// Only the public certificate is returned, never the private key.
///
/// GET /api/v1/pki/ca-public
pub async fn get_ca_public(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    // Get the first organization's CA (for single-tenant deployments)
    // In multi-tenant mode, this would need org identification via header
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        r#"SELECT ca_cert_pem, slug FROM organizations
           WHERE ca_cert_pem IS NOT NULL
           ORDER BY created_at ASC
           LIMIT 1"#,
    )
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((Some(cert_pem), org_slug)) => {
            let fingerprint = appcontrol_common::fingerprint_pem(&cert_pem).unwrap_or_default();
            Ok(Json(json!({
                "ca_cert_pem": cert_pem,
                "fingerprint": fingerprint,
                "organization": org_slug,
            })))
        }
        _ => Err(ApiError::NotFound),
    }
}

// ---------------------------------------------------------------------------
// Server certificate issuance (admin only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IssueServerCertRequest {
    /// Common name for the certificate (e.g., "localhost", "appcontrol.example.com")
    pub common_name: String,
    /// Additional DNS Subject Alternative Names
    #[serde(default)]
    pub san_dns: Vec<String>,
    /// Additional IP Subject Alternative Names
    #[serde(default)]
    pub san_ips: Vec<String>,
    /// Certificate validity in days (default 365)
    pub validity_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct IssueServerCertResponse {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub fingerprint: String,
    pub expires_in_days: u32,
}

/// Issue a server certificate for nginx TLS termination.
///
/// This creates a certificate suitable for HTTPS server authentication,
/// signed by the organization's CA.
///
/// POST /api/v1/pki/server-cert
pub async fn issue_server_cert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<IssueServerCertRequest>,
) -> Result<Json<Value>, ApiError> {
    // Check admin permission
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    crate::error::validate_length("common_name", &req.common_name, 1, 253)?;

    // Load organization CA
    let ca_row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT ca_cert_pem, ca_key_pem FROM organizations WHERE id = $1")
            .bind(crate::db::bind_id(user.organization_id))
            .fetch_optional(&state.db)
            .await?;

    let (ca_cert_pem, ca_key_pem) = match ca_row {
        Some((Some(cert), Some(key))) => (cert, key),
        _ => {
            return Err(ApiError::Validation(
                "Organization CA not initialized. Run PKI init first.".to_string(),
            ));
        }
    };

    let validity_days = req.validity_days.unwrap_or(365);

    // Issue server certificate (same as gateway cert - server auth)
    let issued = appcontrol_common::issue_gateway_cert(
        &ca_cert_pem,
        &ca_key_pem,
        &req.common_name,
        &req.san_dns,
        &req.san_ips,
        validity_days,
    )
    .map_err(|e| ApiError::Internal(format!("Certificate generation failed: {}", e)))?;

    let fingerprint = appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();

    // Log the action
    crate::middleware::audit::log_action(
        &state.db,
        *user.user_id,
        "issue_server_cert",
        "organization",
        *user.organization_id,
        json!({
            "common_name": &req.common_name,
            "san_dns": &req.san_dns,
            "san_ips": &req.san_ips,
            "validity_days": validity_days,
            "fingerprint": &fingerprint,
        }),
    )
    .await
    .ok();

    // Log certificate event
    let cert_expires_at =
        (chrono::Utc::now() + chrono::Duration::days(validity_days as i64)).to_rfc3339();
    #[cfg(feature = "postgres")]
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ('issued', $1, $2, {now}, {now} + $3 * interval '1 day')",
        now = crate::db::sql::now()
    ))
    .bind(&fingerprint)
    .bind(&req.common_name)
    .bind(validity_days as i32)
    .execute(&state.db)
    .await
    .ok();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ('issued', $1, $2, {now}, $3)",
        now = crate::db::sql::now()
    ))
    .bind(&fingerprint)
    .bind(&req.common_name)
    .bind(&cert_expires_at)
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(json!({
        "cert_pem": issued.cert_pem,
        "key_pem": issued.key_pem,
        "ca_pem": issued.ca_pem,
        "fingerprint": fingerprint,
        "expires_in_days": validity_days,
    })))
}

// ---------------------------------------------------------------------------
// Export certificates to volume (admin only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportToVolumeRequest {
    /// Common name for the server certificate
    #[serde(default = "default_common_name")]
    pub common_name: String,
    /// Additional DNS SANs for the server certificate
    #[serde(default)]
    pub san_dns: Vec<String>,
    /// Additional IP SANs for the server certificate
    #[serde(default)]
    pub san_ips: Vec<String>,
    /// Certificate validity in days
    #[serde(default = "default_validity_days")]
    pub validity_days: u32,
}

fn default_common_name() -> String {
    "localhost".to_string()
}

fn default_validity_days() -> u32 {
    365
}

/// Export CA and server certificates to the configured volume path.
///
/// Writes:
/// - /certs/ca.crt (CA certificate)
/// - /certs/server.crt (Server certificate)
/// - /certs/server.key (Server private key)
///
/// POST /api/v1/pki/export-to-volume
pub async fn export_to_volume(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<ExportToVolumeRequest>,
) -> Result<Json<Value>, ApiError> {
    // Check admin permission
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let export_path = std::env::var("CERT_EXPORT_PATH").unwrap_or_else(|_| "/certs".to_string());

    // Load organization CA
    let ca_row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT ca_cert_pem, ca_key_pem FROM organizations WHERE id = $1")
            .bind(crate::db::bind_id(user.organization_id))
            .fetch_optional(&state.db)
            .await?;

    let (ca_cert_pem, ca_key_pem) = match ca_row {
        Some((Some(cert), Some(key))) => (cert, key),
        _ => {
            return Err(ApiError::Validation(
                "Organization CA not initialized. Run PKI init first.".to_string(),
            ));
        }
    };

    // Issue server certificate
    let issued = appcontrol_common::issue_gateway_cert(
        &ca_cert_pem,
        &ca_key_pem,
        &req.common_name,
        &req.san_dns,
        &req.san_ips,
        req.validity_days,
    )
    .map_err(|e| ApiError::Internal(format!("Certificate generation failed: {}", e)))?;

    // Create export directory if it doesn't exist
    std::fs::create_dir_all(&export_path)
        .map_err(|e| ApiError::Internal(format!("Failed to create cert directory: {}", e)))?;

    // Write CA certificate
    let ca_path = format!("{}/ca.crt", export_path);
    std::fs::write(&ca_path, &ca_cert_pem)
        .map_err(|e| ApiError::Internal(format!("Failed to write CA cert: {}", e)))?;

    // Write server certificate
    let cert_path = format!("{}/server.crt", export_path);
    std::fs::write(&cert_path, &issued.cert_pem)
        .map_err(|e| ApiError::Internal(format!("Failed to write server cert: {}", e)))?;

    // Write server key (with restricted permissions on Unix)
    let key_path = format!("{}/server.key", export_path);
    std::fs::write(&key_path, &issued.key_pem)
        .map_err(|e| ApiError::Internal(format!("Failed to write server key: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).ok();
    }

    let fingerprint = appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();

    // Log the action
    crate::middleware::audit::log_action(
        &state.db,
        *user.user_id,
        "export_certs_to_volume",
        "organization",
        *user.organization_id,
        json!({
            "export_path": &export_path,
            "common_name": &req.common_name,
            "fingerprint": &fingerprint,
        }),
    )
    .await
    .ok();

    tracing::info!(
        path = %export_path,
        common_name = %req.common_name,
        fingerprint = %fingerprint,
        "Exported certificates to volume"
    );

    Ok(Json(json!({
        "status": "exported",
        "path": export_path,
        "files": {
            "ca": ca_path,
            "cert": cert_path,
            "key": key_path,
        },
        "fingerprint": fingerprint,
        "expires_in_days": req.validity_days,
    })))
}

// ---------------------------------------------------------------------------
// PKI Status endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PkiStatus {
    pub ca_initialized: bool,
    pub ca_fingerprint: Option<String>,
    pub enrolled_agents: i64,
    pub enrolled_gateways: i64,
    pub pending_rotation: bool,
    pub pending_ca_fingerprint: Option<String>,
    pub rotation_started_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Get PKI status for the organization.
///
/// GET /api/v1/pki/status
#[allow(clippy::type_complexity)]
pub async fn get_pki_status(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    // Get CA status
    let ca_row: Option<(
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> = sqlx::query_as(
        r#"SELECT ca_cert_pem, pending_ca_cert_pem, rotation_started_at
           FROM organizations WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_optional(&state.db)
    .await?;

    let (ca_cert, pending_cert, rotation_started) = ca_row.unwrap_or((None, None, None));

    let ca_fingerprint = ca_cert
        .as_ref()
        .and_then(|c| appcontrol_common::fingerprint_pem(c));

    let pending_ca_fingerprint = pending_cert
        .as_ref()
        .and_then(|c| appcontrol_common::fingerprint_pem(c));

    // Count enrolled entities
    let agent_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM agents WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_one(&state.db)
    .await?;

    let gateway_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM gateways WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "ca_initialized": ca_cert.is_some(),
        "ca_fingerprint": ca_fingerprint,
        "enrolled_agents": agent_count.0,
        "enrolled_gateways": gateway_count.0,
        "pending_rotation": pending_cert.is_some(),
        "pending_ca_fingerprint": pending_ca_fingerprint,
        "rotation_started_at": rotation_started,
    })))
}

// ---------------------------------------------------------------------------
// Auto-export certificates on backend startup
// ---------------------------------------------------------------------------

/// Export PKI CA and gateway certificates to volume if CERT_EXPORT_PATH is configured.
/// Called from main.rs after auto_init_pki().
///
/// Exports:
/// - pki-ca.crt: The PKI CA certificate (for mTLS verification)
/// - gateway.crt: Gateway server certificate (signed by PKI CA)
/// - gateway.key: Gateway private key
pub async fn export_certs_to_volume_if_configured(
    pool: &crate::db::DbPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let export_path = match std::env::var("CERT_EXPORT_PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => return Ok(()), // Not configured, skip
    };

    // Load CA from first organization (single-tenant assumption for dev)
    let ca_row: Option<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, ca_cert_pem, ca_key_pem FROM organizations WHERE ca_cert_pem IS NOT NULL LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let (_org_id, ca_cert_pem, ca_key_pem) = match ca_row {
        Some((id, Some(cert), Some(key))) => (id, cert, key),
        _ => return Ok(()), // No CA yet, skip
    };

    // Create directory
    std::fs::create_dir_all(&export_path)?;

    // Write PKI CA certificate
    let ca_path = format!("{}/pki-ca.crt", export_path);
    std::fs::write(&ca_path, &ca_cert_pem)?;
    tracing::info!(path = %ca_path, "Exported PKI CA certificate to volume");

    // Check if gateway certificate already exists and is valid
    let gateway_cert_path = format!("{}/gateway.crt", export_path);
    let gateway_key_path = format!("{}/gateway.key", export_path);

    let needs_gateway_cert = if std::path::Path::new(&gateway_cert_path).exists() {
        // Check if cert is still valid (expiring in > 30 days)
        match std::fs::read_to_string(&gateway_cert_path) {
            Ok(cert_pem) => {
                // Simple check: if the cert exists and was signed by the same CA, keep it
                // In production, you'd verify expiry and CA fingerprint
                !cert_pem.contains("BEGIN CERTIFICATE")
            }
            Err(_) => true,
        }
    } else {
        true
    };

    if needs_gateway_cert {
        // Generate gateway certificate signed by PKI CA
        // SANs include common Docker/Kubernetes hostnames
        let gateway_cn = std::env::var("GATEWAY_CERT_CN").unwrap_or_else(|_| "gateway".to_string());
        let san_dns = vec![
            "localhost".to_string(),
            "gateway".to_string(),
            "docker-gateway-1".to_string(),
            "appcontrol-gateway".to_string(),
        ];
        let san_ips = vec!["127.0.0.1".to_string()];

        let issued = appcontrol_common::issue_gateway_cert(
            &ca_cert_pem,
            &ca_key_pem,
            &gateway_cn,
            &san_dns,
            &san_ips,
            365, // 1 year validity
        )?;

        // Write gateway certificate
        std::fs::write(&gateway_cert_path, &issued.cert_pem)?;

        // Write gateway key with restricted permissions
        std::fs::write(&gateway_key_path, &issued.key_pem)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gateway_key_path, std::fs::Permissions::from_mode(0o600))
                .ok();
        }

        let fingerprint = appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();
        tracing::info!(
            path = %gateway_cert_path,
            fingerprint = %fingerprint,
            "Generated and exported gateway certificate"
        );

        // Log certificate event
        let gw_expires = (chrono::Utc::now() + chrono::Duration::days(365)).to_rfc3339();
        #[cfg(feature = "postgres")]
        sqlx::query(&format!(
            "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
                 VALUES ('issued', $1, $2, {now}, {now} + interval '365 days')",
            now = crate::db::sql::now()
        ))
        .bind(&fingerprint)
        .bind(&gateway_cn)
        .execute(pool)
        .await
        .ok();

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sqlx::query(&format!(
            "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
                 VALUES ('issued', $1, $2, {now}, $3)",
            now = crate::db::sql::now()
        ))
        .bind(&fingerprint)
        .bind(&gateway_cn)
        .bind(&gw_expires)
        .execute(pool)
        .await
        .ok();
    }

    Ok(())
}

/// Backward-compatible alias for export_certs_to_volume_if_configured.
#[allow(dead_code)]
pub async fn export_ca_to_volume_if_configured(
    pool: &crate::db::DbPool,
    _org_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    export_certs_to_volume_if_configured(pool).await
}

// ---------------------------------------------------------------------------
// Certificate Rotation API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StartRotationRequest {
    /// PEM-encoded new CA certificate
    pub new_ca_cert_pem: String,
    /// PEM-encoded new CA private key
    pub new_ca_key_pem: String,
    /// Grace period in seconds (default 3600 = 1 hour)
    #[serde(default = "default_grace_period")]
    pub grace_period_secs: u64,
}

fn default_grace_period() -> u64 {
    3600
}

/// Start a certificate rotation to migrate to a new CA.
///
/// POST /api/v1/pki/rotation/start
pub async fn start_rotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<StartRotationRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        *user.user_id,
        "start_certificate_rotation",
        "organization",
        *user.organization_id,
        json!({ "grace_period_secs": req.grace_period_secs }),
    )
    .await
    .ok();

    let rotation_id = crate::core::certificate_rotation::start_rotation(
        &state.db,
        *user.organization_id,
        &req.new_ca_cert_pem,
        &req.new_ca_key_pem,
        req.grace_period_secs,
        *user.user_id,
    )
    .await?;

    // Get progress for response
    let progress =
        crate::core::certificate_rotation::get_rotation_progress(&state.db, *user.organization_id)
            .await?;

    Ok(Json(json!({
        "status": "started",
        "rotation_id": rotation_id,
        "progress": progress,
    })))
}

/// Get the current rotation progress.
///
/// GET /api/v1/pki/rotation/progress
pub async fn get_rotation_progress(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let progress =
        crate::core::certificate_rotation::get_rotation_progress(&state.db, *user.organization_id)
            .await?;

    Ok(Json(json!({ "progress": progress })))
}

/// Finalize the certificate rotation.
///
/// POST /api/v1/pki/rotation/finalize
pub async fn finalize_rotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        *user.user_id,
        "finalize_certificate_rotation",
        "organization",
        *user.organization_id,
        json!({}),
    )
    .await
    .ok();

    crate::core::certificate_rotation::finalize_rotation(&state.db, *user.organization_id).await?;

    Ok(Json(json!({ "status": "finalized" })))
}

/// Cancel an in-progress certificate rotation.
///
/// POST /api/v1/pki/rotation/cancel
pub async fn cancel_rotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        *user.user_id,
        "cancel_certificate_rotation",
        "organization",
        *user.organization_id,
        json!({}),
    )
    .await
    .ok();

    crate::core::certificate_rotation::cancel_rotation(&state.db, *user.organization_id).await?;

    Ok(Json(json!({ "status": "cancelled" })))
}

/// Get the CA bundle (current + pending during rotation).
///
/// GET /api/v1/pki/ca-bundle
pub async fn get_ca_bundle(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let bundle =
        crate::core::certificate_rotation::get_ca_bundle(&state.db, *user.organization_id).await?;

    Ok(Json(json!({ "ca_bundle_pem": bundle })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_common_name(), "localhost");
        assert_eq!(default_validity_days(), 365);
        assert_eq!(default_grace_period(), 3600);
    }
}
