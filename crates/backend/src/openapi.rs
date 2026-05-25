//! Code-derived OpenAPI specification.
//!
//! The OpenAPI 3 document for this backend is now generated from the
//! handler annotations themselves (`#[utoipa::path(...)]` on each
//! `pub async fn` in `crate::api::*`) and the `#[derive(utoipa::ToSchema)]`
//! attached to every request/response DTO. There is no longer a static
//! `crates/backend/openapi.json` checked into the repo — the file has
//! been deleted as part of the migration, and `/api/v1/openapi.json` is
//! served at runtime by rendering [`ApiDoc::openapi()`] to JSON.
//!
//! To export the spec without starting the HTTP server (used by
//! `scripts/docs/gen_api.py` and CI), run:
//!
//! ```bash
//! appcontrol-backend --export-openapi /tmp/openapi.json
//! ```
//!
//! ## Coverage
//!
//! This is an incremental migration. The endpoints below are fully
//! annotated; remaining handlers in `crate::api::*` carry a
//! `// TODO(utoipa): not yet annotated` comment and currently produce
//! no `paths` entry — they remain reachable but undocumented until
//! their annotations land. Adding a `#[utoipa::path(...)]` block and
//! listing the handler in `paths(...)` below is all that's needed to
//! bring one in.

use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

use crate::api;
use crate::error::ApiErrorBody;
use appcontrol_common::{
    CheckResult, CheckStatus, CheckType, ClusterHealthPolicy, ClusterMemberConfig, ClusterMode,
    CommandResult, ComponentConfig, ComponentState, ComponentType, DiagnosticRecommendation,
    NativeCommand, OrgRole, PermissionLevel, SwitchoverMode, SwitchoverPhase, UpdateStatus,
};

/// Adds `bearerAuth` (JWT) and `apiKeyAuth` (`X-API-Key` header) security
/// schemes to the generated document so the operation-level
/// `security(...)` references resolve.
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::default);

        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "OIDC/SAML-issued JWT signed with RS256. Pass as `Authorization: Bearer <token>`.",
                    ))
                    .build(),
            ),
        );

        components.add_security_scheme(
            "apiKeyAuth",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-API-Key",
                "Personal or service API key — used by schedulers (Control-M, AutoSys, Dollar Universe, TWS) and the `appctl` CLI.",
            ))),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "AppControl API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Enterprise platform for operational mastery and IT system resilience. \
                       Maps applications as dependency graphs (DAGs), monitors component health, \
                       orchestrates sequenced start/stop/restart operations, and manages DR site failover. \
                       \n\nThis document is generated from the handler source — never hand-edited.",
        license(name = "Proprietary"),
        contact(name = "AppControl Team")
    ),
    servers(
        (url = "/api/v1", description = "AppControl API v1"),
    ),
    security(
        ("bearerAuth" = []),
        ("apiKeyAuth" = []),
    ),
    paths(
        // Applications
        api::apps::list_apps,
        api::apps::get_app,
        api::apps::create_app,
        api::apps::update_app,
        api::apps::delete_app,
        api::apps::start_app,
        api::apps::stop_app,
        api::apps::cancel_operation,
        api::apps::force_unlock_operation,
        api::apps::start_branch,
        api::apps::start_to,
        api::apps::suspend_application,
        api::apps::resume_application,
        api::apps::get_site_overrides,
        // TODO(utoipa): the following handler modules are not yet
        // annotated end-to-end. Once each `pub async fn` carries a
        // `#[utoipa::path(...)]` block, list it here and it will appear
        // in the generated document automatically:
        //   api::components::*, api::permissions::*, api::teams::*,
        //   api::switchover::*, api::diagnostic::*, api::orchestration::*,
        //   api::reports::*, api::history::*, api::variables::*,
        //   api::groups::*, api::links::*, api::command_params::*,
        //   api::topology::*, api::import::*, api::import_wizard::*,
        //   api::profiles::*, api::export::*, api::api_keys::*,
        //   api::agents::*, api::gateways::*, api::sites::*,
        //   api::hostings::*, api::organizations::*, api::users::*,
        //   api::workspaces::*, api::approvals::*, api::break_glass::*,
        //   api::enrollment::*, api::pki_export::*, api::estimates::*,
        //   api::discovery::*, api::schedules::*, api::catalog::*,
        //   api::agent_update::*, api::cluster_members::*,
        //   api::manual_tasks::*, api::logs::*, api::map_settings::*.
    ),
    components(schemas(
        // Error
        ApiErrorBody,
        // Common types (wire format)
        ComponentState,
        PermissionLevel,
        ComponentType,
        CheckType,
        CheckStatus,
        DiagnosticRecommendation,
        ClusterMode,
        ClusterHealthPolicy,
        SwitchoverPhase,
        SwitchoverMode,
        OrgRole,
        UpdateStatus,
        CheckResult,
        CommandResult,
        ComponentConfig,
        ClusterMemberConfig,
        NativeCommand,
        // Apps DTOs
        api::apps::CreateAppRequest,
        api::apps::UpdateAppRequest,
        api::apps::AppRow,
        api::apps::AppWithStatus,
        api::apps::ComponentRow,
        api::apps::DependencyRow,
        api::apps::StartAppRequest,
        api::apps::StopAppRequest,
        api::apps::StartBranchRequest,
        api::apps::StartToRequest,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "Applications", description = "Application CRUD, DAG-sequenced start/stop/restart, suspension."),
        (name = "Components", description = "Per-component CRUD, lifecycle, dependencies, and site overrides."),
        (name = "Permissions", description = "App-level permissions for users/teams + share links."),
        (name = "Teams", description = "Team membership and team-scoped permissions."),
        (name = "Switchover", description = "DR site switchover — 6-phase failover with rollback."),
        (name = "Diagnostic", description = "3-level diagnostic assessment and surgical rebuild."),
        (name = "Reports", description = "DORA-compliant reports: availability, MTTR, RTO, audit, compliance."),
        (name = "Orchestration", description = "Scheduler integration: start/stop/wait-running for Control-M, AutoSys, etc."),
        (name = "Agents", description = "Agent inventory, certificates, and lifecycle."),
        (name = "Gateways", description = "Gateway inventory and lifecycle."),
        (name = "Sites", description = "Sites and hostings (failover topology)."),
        (name = "Users", description = "User management and self-service profile."),
        (name = "Workspaces", description = "Workspace-based site access control."),
        (name = "Discovery", description = "Passive topology discovery and import workflow."),
        (name = "Health", description = "Health, readiness, metrics, OpenAPI."),
    ),
)]
pub struct ApiDoc;

impl ApiDoc {
    /// Render the OpenAPI document to pretty-printed JSON. Used by the
    /// `/api/v1/openapi.json` HTTP handler and the
    /// `--export-openapi <path>` CLI flag.
    pub fn to_pretty_json() -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&Self::openapi())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    /// The generated spec must always serialize. If a `ToSchema` derive or
    /// `#[utoipa::path]` block is malformed, this test fails at runtime
    /// before the bug reaches the documentation pipeline.
    #[test]
    fn api_doc_serializes_to_json() {
        let doc = ApiDoc::openapi();
        let json = serde_json::to_string_pretty(&doc).expect("OpenAPI doc must serialize");
        assert!(!json.is_empty());
        // Sanity: top-level keys we always expect.
        assert!(json.contains("\"openapi\""));
        assert!(json.contains("\"info\""));
        assert!(json.contains("\"paths\""));
    }

    /// At least the `/apps` endpoint family must be present, otherwise the
    /// derive isn't picking up our `#[utoipa::path]` annotations.
    #[test]
    fn api_doc_includes_apps_endpoints() {
        let doc = ApiDoc::openapi();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("/apps"), "spec missing /apps path");
        assert!(
            json.contains("/apps/{id}/start"),
            "spec missing /apps/{{id}}/start path"
        );
    }

    /// The security schemes must be present so per-operation `security(...)`
    /// references resolve.
    #[test]
    fn api_doc_has_security_schemes() {
        let doc = ApiDoc::openapi();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("bearerAuth"));
        assert!(json.contains("apiKeyAuth"));
    }

    #[test]
    fn api_doc_to_pretty_json_works() {
        let s = ApiDoc::to_pretty_json().expect("pretty JSON must succeed");
        assert!(s.starts_with('{'));
    }
}
