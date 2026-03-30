use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{DbPool, DbUuid};
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
        (Fail, NotAvailable, NotAvailable) => Unknown,
        (NotAvailable, _, _) => Unknown,
    }
}

/// Run 3-level diagnosis for all components of an application.
///
/// Uses a single query with ROW_NUMBER() window function to get the latest
/// check result for each (component, check_type) pair, instead of O(3N) queries.
pub async fn diagnose_app(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Vec<ComponentDiagnosis>, DiagnosticError> {
    let app_id: Uuid = app_id.into();
    // Get all components
    let components = sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
    .map_err(|e| DiagnosticError::Database(e.to_string()))?;

    if components.is_empty() {
        return Ok(Vec::new());
    }

    let comp_ids: Vec<DbUuid> = components.iter().map(|(id, _)| *id).collect();

    // Single query: get latest check result per (component_id, check_type)
    let comp_ids_uuid: Vec<Uuid> = comp_ids.iter().map(|id| id.into_inner()).collect();
    let latest_checks = fetch_latest_checks(pool, &comp_ids_uuid)
        .await
        .map_err(|e| DiagnosticError::Database(e.to_string()))?;

    // Build a lookup: (component_id, check_type) → exit_code
    let mut check_map: std::collections::HashMap<(Uuid, &str), i16> =
        std::collections::HashMap::new();
    for (comp_id, check_type, exit_code) in &latest_checks {
        let ct: &str = match check_type.as_str() {
            "health" => "health",
            "integrity" => "integrity",
            "infrastructure" => "infrastructure",
            _ => continue,
        };
        check_map.insert((**comp_id, ct), *exit_code);
    }

    let exit_code_to_status = |code: Option<&i16>| -> CheckStatus {
        match code {
            Some(0) => CheckStatus::Ok,
            Some(_) => CheckStatus::Fail,
            None => CheckStatus::NotAvailable,
        }
    };

    let diagnoses = components
        .into_iter()
        .map(|(comp_id, comp_name)| {
            let health = exit_code_to_status(check_map.get(&(*comp_id, "health")));
            let integrity = exit_code_to_status(check_map.get(&(*comp_id, "integrity")));
            let infrastructure = exit_code_to_status(check_map.get(&(*comp_id, "infrastructure")));
            let recommendation = compute_recommendation(health, integrity, infrastructure);

            ComponentDiagnosis {
                component_id: *comp_id,
                component_name: comp_name,
                health,
                integrity,
                infrastructure,
                recommendation,
            }
        })
        .collect();

    Ok(diagnoses)
}

/// Fetch latest check results for given component IDs
#[cfg(feature = "postgres")]
async fn fetch_latest_checks(
    pool: &DbPool,
    comp_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String, i16)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i16)>(
        r#"
        SELECT component_id, check_type, exit_code
        FROM (
            SELECT component_id, check_type, exit_code,
                   ROW_NUMBER() OVER (PARTITION BY component_id, check_type ORDER BY created_at DESC) as rn
            FROM check_events
            WHERE component_id = ANY($1)
              AND check_type IN ('health', 'integrity', 'infrastructure')
        ) ranked
        WHERE rn = 1
        "#,
    )
    .bind(comp_ids)
    .fetch_all(pool)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_latest_checks(
    pool: &DbPool,
    comp_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String, i16)>, sqlx::Error> {
    if comp_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=comp_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        r#"
        SELECT component_id, check_type, exit_code
        FROM (
            SELECT component_id, check_type, exit_code,
                   ROW_NUMBER() OVER (PARTITION BY component_id, check_type ORDER BY created_at DESC) as rn
            FROM check_events
            WHERE component_id IN ({})
              AND check_type IN ('health', 'integrity', 'infrastructure')
        ) ranked
        WHERE rn = 1
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String, i16)>(&query);
    for id in comp_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String, i16)> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id_str, check_type, exit_code)| {
            Uuid::parse_str(&id_str)
                .ok()
                .map(|id| (DbUuid::from(id), check_type, exit_code))
        })
        .collect())
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
