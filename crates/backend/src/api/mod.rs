pub mod agents;
pub mod apps;
pub mod components;
pub mod diagnostic;
pub mod health;
pub mod orchestration;
pub mod permissions;
pub mod reports;
pub mod switchover;
pub mod teams;

use axum::{
    middleware as axum_middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use crate::AppState;

pub fn api_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Applications
        .route("/apps", get(apps::list_apps).post(apps::create_app))
        .route(
            "/apps/{id}",
            get(apps::get_app)
                .put(apps::update_app)
                .delete(apps::delete_app),
        )
        .route("/apps/{id}/start", post(apps::start_app))
        .route("/apps/{id}/stop", post(apps::stop_app))
        .route("/apps/{id}/start-branch", post(apps::start_branch))
        // Components
        .route(
            "/apps/{app_id}/components",
            get(components::list_components).post(components::create_component),
        )
        .route(
            "/components/{id}",
            get(components::get_component)
                .put(components::update_component)
                .delete(components::delete_component),
        )
        .route("/components/{id}/start", post(components::start_component))
        .route("/components/{id}/stop", post(components::stop_component))
        .route(
            "/components/{id}/command/{cmd}",
            post(components::execute_command),
        )
        // Dependencies
        .route(
            "/apps/{app_id}/dependencies",
            get(components::list_dependencies).post(components::create_dependency),
        )
        .route("/dependencies/{id}", delete(components::delete_dependency))
        // Permissions
        .route(
            "/apps/{app_id}/permissions/users",
            get(permissions::list_user_permissions).post(permissions::grant_user_permission),
        )
        .route(
            "/apps/{app_id}/permissions/teams",
            get(permissions::list_team_permissions).post(permissions::grant_team_permission),
        )
        .route(
            "/apps/{app_id}/permissions/share-links",
            get(permissions::list_share_links).post(permissions::create_share_link),
        )
        .route(
            "/apps/{app_id}/permissions/effective",
            get(permissions::get_effective_permission),
        )
        // Teams
        .route("/teams", get(teams::list_teams).post(teams::create_team))
        .route(
            "/teams/{id}",
            get(teams::get_team)
                .put(teams::update_team)
                .delete(teams::delete_team),
        )
        .route(
            "/teams/{id}/members",
            get(teams::list_members).post(teams::add_member),
        )
        .route(
            "/teams/{id}/members/{user_id}",
            delete(teams::remove_member),
        )
        // Switchover
        .route(
            "/apps/{app_id}/switchover",
            post(switchover::start_switchover),
        )
        .route(
            "/apps/{app_id}/switchover/next-phase",
            post(switchover::next_phase),
        )
        .route(
            "/apps/{app_id}/switchover/rollback",
            post(switchover::rollback),
        )
        .route("/apps/{app_id}/switchover/commit", post(switchover::commit))
        .route("/apps/{app_id}/switchover/status", get(switchover::status))
        // Diagnostic & Rebuild
        .route("/apps/{app_id}/diagnose", post(diagnostic::diagnose))
        .route("/apps/{app_id}/rebuild", post(diagnostic::rebuild))
        // Reports
        .route(
            "/apps/{app_id}/reports/availability",
            get(reports::availability),
        )
        .route("/apps/{app_id}/reports/incidents", get(reports::incidents))
        .route(
            "/apps/{app_id}/reports/switchovers",
            get(reports::switchovers),
        )
        .route("/apps/{app_id}/reports/audit", get(reports::audit))
        .route(
            "/apps/{app_id}/reports/compliance",
            get(reports::compliance),
        )
        .route("/apps/{app_id}/reports/rto", get(reports::rto))
        // Orchestration (scheduler)
        .route(
            "/orchestration/apps/{app_id}/start",
            post(orchestration::start),
        )
        .route(
            "/orchestration/apps/{app_id}/stop",
            post(orchestration::stop),
        )
        .route(
            "/orchestration/apps/{app_id}/status",
            get(orchestration::status),
        )
        .route(
            "/orchestration/apps/{app_id}/wait-running",
            get(orchestration::wait_running),
        )
        // Agents
        .route("/agents", get(agents::list_agents))
        .route("/agents/{id}", get(agents::get_agent))
        .route_layer(axum_middleware::from_fn_with_state(
            state,
            crate::middleware::auth::auth_middleware,
        ))
}
