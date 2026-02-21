use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use appcontrol_common::{CheckStatus, DiagnosticRecommendation};

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticError {
    #[error("Database error: {0}")]
    Database(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDiagnosis {
    pub component_id: Uuid,
    pub component_name: String,
    pub health: CheckStatus,
    pub integrity: CheckStatus,
    pub infrastructure: CheckStatus,
    pub recommendation: DiagnosticRecommendation,
}

/// Decision matrix for diagnostic recommendation.
///
/// H=OK, I=OK, Inf=OK    → Healthy
/// H=OK, I=OK, Inf=FAIL  → Healthy (warn infra)
/// H=OK, I=FAIL, Inf=OK  → Healthy (integrity warning, not actionable)
/// H=FAIL, I=OK, Inf=OK  → Restart
/// H=FAIL, I=FAIL, Inf=OK → AppRebuild
/// H=FAIL, *, Inf=FAIL    → InfraRebuild
/// N/A (agent down)        → Unknown
pub fn compute_recommendation(
    health: CheckStatus,
    integrity: CheckStatus,
    infrastructure: CheckStatus,
) -> DiagnosticRecommendation {
    use CheckStatus::*;
    use DiagnosticRecommendation::*;

    if matches!(health, NotAvailable)
        && matches!(integrity, NotAvailable)
        && matches!(infrastructure, NotAvailable)
    {
        return Unknown;
    }

    match (health, integrity, infrastructure) {
        (Ok, Ok, Ok) => Healthy,
        (Ok, Ok, Fail) => Healthy, // warn infra but healthy
        (Ok, Fail, Ok) => Healthy, // integrity warning
        (Ok, _, _) => Healthy,
        (Fail, _, Fail) => InfraRebuild,
        (Fail, Fail, Ok) => AppRebuild,
        (Fail, Fail, NotAvailable) => AppRebuild,
        (Fail, Ok, Ok) => Restart,
        (Fail, Ok, NotAvailable) => Restart,
        (Fail, NotAvailable, Ok) => Restart,
        (Fail, NotAvailable, Fail) => InfraRebuild,
        (Fail, NotAvailable, NotAvailable) => Unknown,
        (NotAvailable, _, _) => Unknown,
    }
}

/// Run 3-level diagnosis for all components of an application.
pub async fn diagnose_app(
    pool: &sqlx::PgPool,
    app_id: Uuid,
) -> Result<Vec<ComponentDiagnosis>, DiagnosticError> {
    let components = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DiagnosticError::Database(e.to_string()))?;

    let mut diagnoses = Vec::new();

    for (comp_id, comp_name) in components {
        let health = get_latest_check_status(pool, comp_id, "health").await?;
        let integrity = get_latest_check_status(pool, comp_id, "integrity").await?;
        let infrastructure = get_latest_check_status(pool, comp_id, "infrastructure").await?;

        let recommendation = compute_recommendation(health, integrity, infrastructure);

        diagnoses.push(ComponentDiagnosis {
            component_id: comp_id,
            component_name: comp_name,
            health,
            integrity,
            infrastructure,
            recommendation,
        });
    }

    Ok(diagnoses)
}

async fn get_latest_check_status(
    pool: &sqlx::PgPool,
    component_id: Uuid,
    check_type: &str,
) -> Result<CheckStatus, DiagnosticError> {
    let result = sqlx::query_scalar::<_, i16>(
        r#"
        SELECT exit_code
        FROM check_events
        WHERE component_id = $1 AND check_type = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(component_id)
    .bind(check_type)
    .fetch_optional(pool)
    .await
    .map_err(|e| DiagnosticError::Database(e.to_string()))?;

    Ok(match result {
        Some(0) => CheckStatus::Ok,
        Some(_) => CheckStatus::Fail,
        None => CheckStatus::NotAvailable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use CheckStatus::*;
    use DiagnosticRecommendation::*;

    #[test]
    fn test_all_ok_healthy() {
        assert_eq!(compute_recommendation(Ok, Ok, Ok), Healthy);
    }

    #[test]
    fn test_health_ok_infra_fail_healthy() {
        assert_eq!(compute_recommendation(Ok, Ok, Fail), Healthy);
    }

    #[test]
    fn test_health_ok_integrity_fail_healthy() {
        assert_eq!(compute_recommendation(Ok, Fail, Ok), Healthy);
    }

    #[test]
    fn test_health_fail_rest_ok_restart() {
        assert_eq!(compute_recommendation(Fail, Ok, Ok), Restart);
    }

    #[test]
    fn test_health_fail_integrity_fail_app_rebuild() {
        assert_eq!(compute_recommendation(Fail, Fail, Ok), AppRebuild);
    }

    #[test]
    fn test_health_fail_infra_fail_infra_rebuild() {
        assert_eq!(compute_recommendation(Fail, Ok, Fail), InfraRebuild);
    }

    #[test]
    fn test_health_fail_both_fail_infra_rebuild() {
        assert_eq!(compute_recommendation(Fail, Fail, Fail), InfraRebuild);
    }

    #[test]
    fn test_all_na_unknown() {
        assert_eq!(
            compute_recommendation(NotAvailable, NotAvailable, NotAvailable),
            Unknown
        );
    }
}
