//! Activation level — graduated adoption ladder per application.
//!
//! See `migrations/V056__application_activation_level.sql` for the model
//! and `docs/methodology.html` § Phase 4 for the conceptual ladder.
//!
//! The level controls which operations the platform is allowed to perform
//! on an application:
//!
//! | Level | Name             | Read map | Run checks | Start / stop / rebuild        |
//! |-------|------------------|----------|------------|-------------------------------|
//! | 0     | Captation        | yes      | no         | refused                       |
//! | 1     | Advisory         | yes      | no         | refused                       |
//! | 2     | Diagnostic       | yes      | yes        | refused                       |
//! | 3     | Ops sous PR      | yes      | yes        | allowed if PR-approved header |
//! | 4     | Ops directes     | yes      | yes        | allowed (RBAC still applies)  |
//!
//! Enforcement helpers below should be called by every handler that mutates
//! the runtime state of an application (start, stop, restart, branch start,
//! rebuild, switchover) BEFORE acquiring the operation lock or building the
//! plan.

use uuid::Uuid;

use crate::db::DbPool;
use crate::error::ApiError;

/// Numerical activation level. Stored as SMALLINT in `applications.activation_level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActivationLevel {
    Captation = 0,
    Advisory = 1,
    Diagnostic = 2,
    PrOnly = 3,
    Direct = 4,
}

impl ActivationLevel {
    pub fn from_i16(value: i16) -> Result<Self, ApiError> {
        match value {
            0 => Ok(ActivationLevel::Captation),
            1 => Ok(ActivationLevel::Advisory),
            2 => Ok(ActivationLevel::Diagnostic),
            3 => Ok(ActivationLevel::PrOnly),
            4 => Ok(ActivationLevel::Direct),
            other => Err(ApiError::Internal(format!(
                "invalid activation_level {} stored in database",
                other
            ))),
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }

    pub fn name(self) -> &'static str {
        match self {
            ActivationLevel::Captation => "captation",
            ActivationLevel::Advisory => "advisory",
            ActivationLevel::Diagnostic => "diagnostic",
            ActivationLevel::PrOnly => "pr-only",
            ActivationLevel::Direct => "direct",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ActivationLevel::Captation => {
                "Captation seule depuis les référentiels — aucune opération possible."
            }
            ActivationLevel::Advisory => {
                "Agents en observation seule — aucun check exécuté, aucune opération."
            }
            ActivationLevel::Diagnostic => {
                "Checks 3 niveaux actifs — état temps réel connu, aucun start/stop."
            }
            ActivationLevel::PrOnly => {
                "Opérations autorisées via Pull Request mergée (header X-PR-Approved-Sha)."
            }
            ActivationLevel::Direct => {
                "Opérations directes pour les rôles habilités (RBAC s'applique)."
            }
        }
    }
}

/// Result of an activation-level check on a runtime operation request.
pub enum ActivationDecision {
    /// Operation may proceed without further checks.
    Allow,
    /// Operation requires a merged-PR approval reference (provided e.g. by
    /// header `X-PR-Approved-Sha` on REST, or `pr_approved_sha` field on
    /// scheduler integration calls). The handler must verify that the
    /// caller provided one before proceeding.
    RequirePrApproval,
}

/// Fetch the current activation level for an application.
pub async fn get_application_level(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<ActivationLevel, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<(i16,)> = sqlx::query_as(
        "SELECT activation_level FROM applications WHERE id = $1",
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(i16,)> = sqlx::query_as(
        "SELECT activation_level FROM applications WHERE id = ?",
    )
    .bind(crate::db::DbUuid::from(app_id))
    .fetch_optional(pool)
    .await?;

    let level = row.ok_or(ApiError::NotFound)?.0;
    ActivationLevel::from_i16(level)
}

/// Authoritative check before any state-mutating operation on an application
/// (start, stop, restart, rebuild, branch start, switchover, etc.).
///
/// Returns `Ok(Allow)` if the operation may proceed directly, or
/// `Ok(RequirePrApproval)` if the handler must then verify a PR approval
/// reference before continuing.
///
/// Returns `Err(Forbidden)` with a precise message when the application's
/// activation level forbids the operation entirely.
pub async fn require_runtime_ops(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<ActivationDecision, ApiError> {
    let level = get_application_level(pool, app_id).await?;
    match level {
        ActivationLevel::Captation
        | ActivationLevel::Advisory
        | ActivationLevel::Diagnostic => Err(ApiError::Forbidden),
        ActivationLevel::PrOnly => Ok(ActivationDecision::RequirePrApproval),
        ActivationLevel::Direct => Ok(ActivationDecision::Allow),
    }
}

/// Authoritative check before any check execution request (e.g. on-demand
/// diagnostic). Diagnostic and above are allowed.
pub async fn require_diagnostic(pool: &DbPool, app_id: Uuid) -> Result<(), ApiError> {
    let level = get_application_level(pool, app_id).await?;
    if level >= ActivationLevel::Diagnostic {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// HTTP header carrying a PR approval reference for operations executed
/// under PR-only mode (level 3).
pub const PR_APPROVAL_HEADER: &str = "X-PR-Approved-Sha";

/// One-shot check used by REST handlers that perform a runtime operation.
///
/// `pr_approved_sha` is the value of the `X-PR-Approved-Sha` header, if any.
/// It is required (and validated as non-empty) when the application sits at
/// level 3 (PR-only); ignored otherwise.
///
/// On refusal returns a `Forbidden` with a message that identifies the
/// reason so the caller knows exactly what to fix.
pub async fn check_runtime_ops_allowed(
    pool: &DbPool,
    app_id: Uuid,
    pr_approved_sha: Option<&str>,
) -> Result<(), ApiError> {
    match require_runtime_ops(pool, app_id).await? {
        ActivationDecision::Allow => Ok(()),
        ActivationDecision::RequirePrApproval => match pr_approved_sha {
            Some(sha) if !sha.trim().is_empty() => {
                tracing::info!(
                    application_id = %app_id,
                    pr_sha = sha,
                    "PR-only operation accepted with approval reference"
                );
                Ok(())
            }
            _ => Err(ApiError::Forbidden),
        },
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn pr_approval_header_constant_is_canonical() {
        // Documented across vision.html § A5 and methodology.html, so the
        // exact spelling matters for external integrators.
        assert_eq!(PR_APPROVAL_HEADER, "X-PR-Approved-Sha");
    }
}

/// Update an application's activation level. The caller is responsible for
/// checking RBAC (manage permission or org admin) before calling this.
pub async fn set_application_level(
    pool: &DbPool,
    app_id: Uuid,
    new_level: ActivationLevel,
) -> Result<(), ApiError> {
    #[cfg(feature = "postgres")]
    let affected = sqlx::query(
        "UPDATE applications SET activation_level = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(new_level.as_i16())
    .bind(app_id)
    .execute(pool)
    .await?
    .rows_affected();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let affected = sqlx::query(
        "UPDATE applications SET activation_level = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(new_level.as_i16())
    .bind(crate::db::DbUuid::from(app_id))
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ordering_matches_capability_inclusion() {
        // Higher level = more capabilities. The PartialOrd must reflect this so
        // `level >= ActivationLevel::Diagnostic` is the correct way to ask
        // "diagnostic and above".
        assert!(ActivationLevel::Direct > ActivationLevel::PrOnly);
        assert!(ActivationLevel::PrOnly > ActivationLevel::Diagnostic);
        assert!(ActivationLevel::Diagnostic > ActivationLevel::Advisory);
        assert!(ActivationLevel::Advisory > ActivationLevel::Captation);
    }

    #[test]
    fn from_i16_round_trips() {
        for raw in 0..=4_i16 {
            let lvl = ActivationLevel::from_i16(raw).expect("valid in-range value");
            assert_eq!(lvl.as_i16(), raw);
        }
    }

    #[test]
    fn from_i16_rejects_out_of_range() {
        assert!(ActivationLevel::from_i16(-1).is_err());
        assert!(ActivationLevel::from_i16(5).is_err());
        assert!(ActivationLevel::from_i16(99).is_err());
    }

    #[test]
    fn names_and_descriptions_are_distinct() {
        let levels = [
            ActivationLevel::Captation,
            ActivationLevel::Advisory,
            ActivationLevel::Diagnostic,
            ActivationLevel::PrOnly,
            ActivationLevel::Direct,
        ];
        let names: Vec<&str> = levels.iter().map(|l| l.name()).collect();
        let descs: Vec<&str> = levels.iter().map(|l| l.description()).collect();
        assert_eq!(names.iter().collect::<std::collections::HashSet<_>>().len(), 5);
        assert_eq!(descs.iter().collect::<std::collections::HashSet<_>>().len(), 5);
    }
}
