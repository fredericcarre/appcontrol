pub mod agent_update;
pub mod agents;
pub mod api_keys;
pub mod approvals;
pub mod apps;
pub mod break_glass;
pub mod command_params;
pub mod components;
pub mod diagnostic;
pub mod discovery;
pub mod enrollment;
pub mod estimates;
pub mod export;
pub mod gateways;
pub mod groups;
pub mod health;
pub mod history;
pub mod import;
pub mod import_wizard;
pub mod links;
pub mod logs;
pub mod orchestration;
pub mod organizations;
pub mod permissions;
pub mod pki_export;
pub mod profiles;
pub mod reports;
pub mod schedules;
pub mod sites;
pub mod switchover;
pub mod teams;
pub mod topology;
pub mod users;
pub mod variables;
pub mod workspaces;

use axum::{
    middleware as axum_middleware,
    routing::{delete, get, patch, post, put},
    Router,
};
use std::sync::Arc;

use crate::AppState;

pub fn api_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Applications
        .route("/apps", get(apps::list_apps).post(apps::create_app))
        .route(
            "/apps/:id",
            get(apps::get_app)
                .put(apps::update_app)
                .delete(apps::delete_app),
        )
        .route("/apps/:id/start", post(apps::start_app))
        .route("/apps/:id/stop", post(apps::stop_app))
        .route("/apps/:id/start-branch", post(apps::start_branch))
        .route("/apps/:id/start-to", post(apps::start_to))
        .route("/apps/:id/cancel", post(apps::cancel_operation))
        .route("/apps/:id/force-unlock", post(apps::force_unlock_operation))
        .route("/apps/:id/suspend", put(apps::suspend_application))
        .route("/apps/:id/resume", put(apps::resume_application))
        .route(
            "/apps/:app_id/site-overrides",
            get(apps::get_site_overrides),
        )
        // Components
        .route(
            "/apps/:app_id/components",
            get(components::list_components).post(components::create_component),
        )
        .route(
            "/components/:id",
            get(components::get_component)
                .put(components::update_component)
                .delete(components::delete_component),
        )
        .route("/components/:id/start", post(components::start_component))
        .route("/components/:id/stop", post(components::stop_component))
        .route(
            "/components/:id/force-stop",
            post(components::force_stop_component),
        )
        .route(
            "/components/:id/start-with-deps",
            post(components::start_with_deps),
        )
        .route(
            "/components/:id/restart-with-dependents",
            post(components::restart_with_dependents),
        )
        .route(
            "/components/:id/command/:cmd",
            post(components::execute_command),
        )
        .route(
            "/components/:id/metrics",
            get(components::get_component_metrics),
        )
        .route(
            "/components/:id/metrics/history",
            get(components::get_component_metrics_history),
        )
        .route(
            "/components/:id/commands",
            get(components::list_custom_commands),
        )
        .route(
            "/components/:id/command-executions",
            get(components::list_command_executions),
        )
        .route(
            "/components/:id/state-transitions",
            get(components::list_state_transitions),
        )
        .route(
            "/components/:id/check-events",
            get(components::list_check_events),
        )
        // Component log sources
        .route(
            "/components/:id/log-sources",
            get(logs::list_log_sources).post(logs::create_log_source),
        )
        .route(
            "/log-sources/:id",
            put(logs::update_log_source).delete(logs::delete_log_source),
        )
        .route("/components/:id/logs", get(logs::get_component_logs))
        .route(
            "/components/:id/logs/command/:name",
            post(logs::run_diagnostic_command),
        )
        // Component site overrides (failover configuration)
        .route(
            "/components/:id/site-overrides",
            get(components::list_site_overrides),
        )
        .route(
            "/components/:id/site-overrides/:site_id",
            put(components::upsert_site_override).delete(components::delete_site_override),
        )
        // Component positions (for map designer)
        .route(
            "/components/:id/position",
            patch(components::update_position),
        )
        .route(
            "/components/batch-positions",
            patch(components::update_positions_batch),
        )
        // Dependencies
        .route(
            "/apps/:app_id/dependencies",
            get(components::list_dependencies).post(components::create_dependency),
        )
        .route("/dependencies/:id", delete(components::delete_dependency))
        // Permissions
        .route(
            "/apps/:app_id/permissions/users",
            get(permissions::list_user_permissions).post(permissions::grant_user_permission),
        )
        .route(
            "/apps/:app_id/permissions/teams",
            get(permissions::list_team_permissions).post(permissions::grant_team_permission),
        )
        .route(
            "/apps/:app_id/permissions",
            get(permissions::list_all_permissions),
        )
        .route(
            "/apps/:app_id/permissions/:perm_id",
            delete(permissions::delete_permission),
        )
        .route(
            "/apps/:app_id/permissions/share-links",
            get(permissions::list_share_links).post(permissions::create_share_link),
        )
        .route(
            "/apps/:app_id/permissions/share-links/:link_id",
            delete(permissions::revoke_share_link),
        )
        .route(
            "/apps/:app_id/permissions/effective",
            get(permissions::get_effective_permission),
        )
        // User search / discovery
        .route("/users/search", get(permissions::search_users))
        // Share link consumption
        .route(
            "/share-links/consume",
            post(permissions::consume_share_link),
        )
        // Teams
        .route("/teams", get(teams::list_teams).post(teams::create_team))
        .route(
            "/teams/:id",
            get(teams::get_team)
                .put(teams::update_team)
                .delete(teams::delete_team),
        )
        .route(
            "/teams/:id/members",
            get(teams::list_members).post(teams::add_member),
        )
        .route("/teams/:id/members/:user_id", delete(teams::remove_member))
        // Switchover
        .route(
            "/apps/:app_id/switchover",
            post(switchover::start_switchover),
        )
        .route(
            "/apps/:app_id/switchover/next-phase",
            post(switchover::next_phase),
        )
        .route(
            "/apps/:app_id/switchover/rollback",
            post(switchover::rollback),
        )
        .route("/apps/:app_id/switchover/commit", post(switchover::commit))
        .route("/apps/:app_id/switchover/status", get(switchover::status))
        // Diagnostic & Rebuild
        .route("/apps/:app_id/diagnose", post(diagnostic::diagnose))
        .route("/apps/:app_id/rebuild", post(diagnostic::rebuild))
        // Reports
        .route(
            "/apps/:app_id/reports/availability",
            get(reports::availability),
        )
        .route("/apps/:app_id/reports/incidents", get(reports::incidents))
        .route(
            "/apps/:app_id/reports/switchovers",
            get(reports::switchovers),
        )
        .route("/apps/:app_id/reports/pra", get(reports::drp_report))
        .route("/apps/:app_id/reports/audit", get(reports::audit))
        .route("/apps/:app_id/reports/compliance", get(reports::compliance))
        .route("/apps/:app_id/reports/rto", get(reports::rto))
        .route("/apps/:app_id/reports/mttr", get(reports::mttr))
        // Global audit log (org-level, all apps)
        .route("/reports/audit", get(reports::global_audit))
        .route("/apps/:app_id/activity", get(reports::activity_feed))
        .route("/apps/:app_id/health-summary", get(reports::health_summary))
        // History (Time Machine)
        .route("/apps/:app_id/history", get(history::app_history))
        // Orchestration (scheduler)
        .route(
            "/orchestration/apps/:app_id/start",
            post(orchestration::start),
        )
        .route(
            "/orchestration/apps/:app_id/stop",
            post(orchestration::stop),
        )
        .route(
            "/orchestration/apps/:app_id/status",
            get(orchestration::status),
        )
        .route(
            "/orchestration/apps/:app_id/wait-running",
            get(orchestration::wait_running),
        )
        .route(
            "/orchestration/apps/:app_id/health",
            get(orchestration::health),
        )
        .route(
            "/orchestration/apps/:app_id/preflight",
            get(orchestration::preflight),
        )
        // Variables
        .route(
            "/apps/:app_id/variables",
            get(variables::list_variables).post(variables::create_variable),
        )
        .route(
            "/apps/:app_id/variables/:var_id",
            put(variables::update_variable).delete(variables::delete_variable),
        )
        // Component Groups
        .route(
            "/apps/:app_id/groups",
            get(groups::list_groups).post(groups::create_group),
        )
        .route(
            "/apps/:app_id/groups/:group_id",
            put(groups::update_group).delete(groups::delete_group),
        )
        // Component Links
        .route(
            "/components/:component_id/links",
            get(links::list_links).post(links::create_link),
        )
        .route(
            "/components/:component_id/links/:link_id",
            put(links::update_link).delete(links::delete_link),
        )
        // Command Input Parameters
        .route(
            "/commands/:command_id/params",
            get(command_params::list_params).post(command_params::create_param),
        )
        .route(
            "/commands/:command_id/params/:param_id",
            delete(command_params::delete_param),
        )
        // Topology export, plan, validation, dependency history
        .route("/apps/:app_id/topology", get(topology::get_topology))
        .route("/apps/:app_id/plan", get(topology::get_plan))
        .route(
            "/apps/:app_id/validate-sequence",
            post(topology::validate_sequence),
        )
        .route(
            "/apps/:app_id/dependency-history",
            get(topology::dependency_history),
        )
        // Map Import (YAML v3 legacy, JSON v4 native)
        .route("/import/yaml", post(import::import_yaml_map))
        .route("/import/json", post(import::import_json_map))
        .route("/import/fetch-url", post(import::fetch_url))
        // Enhanced Import Wizard (with gateway resolution + binding profiles)
        .route("/import/preview", post(import_wizard::preview_import))
        .route("/import/execute", post(import_wizard::execute_import))
        // Binding Profiles
        .route(
            "/apps/:app_id/profiles",
            get(profiles::list_profiles).post(profiles::create_profile),
        )
        .route(
            "/apps/:app_id/profiles/:name",
            get(profiles::get_profile).delete(profiles::delete_profile),
        )
        .route(
            "/apps/:app_id/profiles/:name/activate",
            put(profiles::activate_profile),
        )
        // DR Pattern Rules
        .route(
            "/dr-pattern-rules",
            get(profiles::list_dr_pattern_rules).post(profiles::create_dr_pattern_rule),
        )
        .route(
            "/dr-pattern-rules/:id",
            put(profiles::update_dr_pattern_rule).delete(profiles::delete_dr_pattern_rule),
        )
        // JSON Export
        .route("/apps/:app_id/export", get(export::export_app_json))
        // API Keys
        .route(
            "/api-keys",
            get(api_keys::list_api_keys).post(api_keys::create_api_key),
        )
        .route("/api-keys/:id", delete(api_keys::delete_api_key))
        // Agents
        .route("/agents", get(agents::list_agents))
        .route("/agents/bulk-delete", post(agents::bulk_delete_agents))
        .route(
            "/agents/:id",
            get(agents::get_agent).delete(agents::delete_agent),
        )
        .route("/agents/:id/block", post(agents::block_agent))
        .route("/agents/:id/unblock", post(agents::unblock_agent))
        .route("/agents/:id/metrics", get(agents::get_agent_metrics))
        .route("/agents/:id/revoke-cert", post(gateways::revoke_agent_cert))
        // Gateways
        .route("/gateways", get(gateways::list_gateways))
        .route(
            "/gateways/:id",
            get(gateways::get_gateway)
                .put(gateways::update_gateway)
                .delete(gateways::delete_gateway),
        )
        .route("/gateways/:id/agents", get(gateways::list_gateway_agents))
        .route("/gateways/:id/suspend", post(gateways::suspend_gateway))
        .route("/gateways/:id/activate", post(gateways::activate_gateway))
        .route("/gateways/:id/block", post(gateways::block_gateway))
        .route(
            "/gateways/:id/set-primary",
            post(gateways::set_gateway_primary),
        )
        .route(
            "/gateways/:id/revoke-cert",
            post(gateways::revoke_gateway_cert),
        )
        .route(
            "/revoked-certificates",
            get(gateways::list_revoked_certificates),
        )
        // Sites
        .route("/sites", get(sites::list_sites).post(sites::create_site))
        .route(
            "/sites/:id",
            get(sites::get_site)
                .put(sites::update_site)
                .delete(sites::delete_site),
        )
        // Organizations (super-admin)
        .route(
            "/organizations",
            get(organizations::list_organizations).post(organizations::create_organization),
        )
        .route(
            "/organizations/:id",
            get(organizations::get_organization).put(organizations::update_organization),
        )
        // Users
        .route("/users", get(users::list_users).post(users::create_user))
        .route("/users/me", get(users::get_me))
        .route("/users/me/password", post(users::change_my_password))
        .route("/users/:id", get(users::get_user).put(users::update_user))
        // Workspaces (site/zone access control)
        .route(
            "/workspaces",
            get(workspaces::list_workspaces).post(workspaces::create_workspace),
        )
        .route("/workspaces/:id", delete(workspaces::delete_workspace))
        .route(
            "/workspaces/:id/sites",
            get(workspaces::list_workspace_sites).post(workspaces::add_workspace_site),
        )
        .route(
            "/workspaces/:id/sites/:site_id",
            delete(workspaces::remove_workspace_site),
        )
        .route(
            "/workspaces/:id/members",
            get(workspaces::list_workspace_members).post(workspaces::add_workspace_member),
        )
        .route(
            "/workspaces/:id/members/:member_id",
            delete(workspaces::remove_workspace_member),
        )
        .route("/workspaces/my-sites", get(workspaces::my_accessible_sites))
        // Approval Workflows (4-eyes principle)
        .route(
            "/approvals",
            get(approvals::list_approval_requests).post(approvals::create_approval_request),
        )
        .route("/approvals/:id/decide", post(approvals::decide_approval))
        .route(
            "/approvals/policies",
            get(approvals::list_approval_policies).post(approvals::upsert_approval_policy),
        )
        // Break-Glass Emergency Access (admin endpoints)
        .route(
            "/break-glass/accounts",
            get(break_glass::list_break_glass_accounts)
                .post(break_glass::create_break_glass_account),
        )
        .route(
            "/break-glass/sessions",
            get(break_glass::list_break_glass_sessions),
        )
        .route(
            "/break-glass/sessions/:id/end",
            post(break_glass::end_break_glass_session),
        )
        // Enrollment token management (authenticated admin endpoints)
        .route(
            "/enrollment/tokens",
            get(enrollment::list_enrollment_tokens).post(enrollment::create_enrollment_token),
        )
        .route(
            "/enrollment/tokens/:id/revoke",
            post(enrollment::revoke_enrollment_token),
        )
        .route(
            "/enrollment/events",
            get(enrollment::list_enrollment_events),
        )
        .route("/enrollment/config", get(enrollment::get_enrollment_config))
        // PKI management
        .route("/pki/init", post(enrollment::init_pki))
        .route("/pki/import", post(enrollment::import_pki))
        .route("/pki/ca", get(enrollment::get_ca_cert))
        .route("/pki/ca-bundle", get(pki_export::get_ca_bundle))
        .route("/pki/status", get(pki_export::get_pki_status))
        .route("/pki/server-cert", post(pki_export::issue_server_cert))
        .route("/pki/export-to-volume", post(pki_export::export_to_volume))
        // Certificate rotation
        .route("/pki/rotation/start", post(pki_export::start_rotation))
        .route(
            "/pki/rotation/progress",
            get(pki_export::get_rotation_progress),
        )
        .route(
            "/pki/rotation/finalize",
            post(pki_export::finalize_rotation),
        )
        .route("/pki/rotation/cancel", post(pki_export::cancel_rotation))
        // SAML group mapping admin API (requires auth)
        .merge(crate::auth::saml::saml_admin_routes())
        // PDF report export
        .route("/apps/:app_id/reports/export", get(reports::export_pdf))
        // Operation time estimates
        .route("/apps/:app_id/estimates", get(estimates::get_estimates))
        // Discovery: passive topology scanning + multi-step workflow
        .route("/discovery/reports", get(discovery::list_reports))
        .route("/discovery/reports/:id", get(discovery::get_report))
        .route(
            "/discovery/trigger/:agent_id",
            post(discovery::trigger_scan),
        )
        .route("/discovery/trigger-all", post(discovery::trigger_all))
        .route("/discovery/correlate", post(discovery::correlate))
        .route(
            "/discovery/drafts",
            get(discovery::list_drafts).post(discovery::create_draft),
        )
        .route("/discovery/drafts/:id", get(discovery::get_draft))
        .route(
            "/discovery/drafts/:id/components",
            put(discovery::update_draft_components),
        )
        .route(
            "/discovery/drafts/:id/dependencies",
            put(discovery::update_draft_dependencies),
        )
        .route("/discovery/drafts/:id/apply", post(discovery::apply_draft))
        // Scheduled snapshots
        .route(
            "/discovery/schedules",
            get(discovery::list_schedules).post(discovery::create_schedule),
        )
        .route(
            "/discovery/schedules/:id",
            patch(discovery::update_schedule).delete(discovery::delete_schedule),
        )
        .route("/discovery/snapshots", get(discovery::list_snapshots))
        .route(
            "/discovery/snapshots/compare",
            post(discovery::compare_snapshots),
        )
        .route(
            "/discovery/file-content",
            post(discovery::read_file_content),
        )
        // Operation Schedules (start/stop/restart automation)
        .route(
            "/apps/:app_id/schedules",
            get(schedules::list_app_schedules).post(schedules::create_app_schedule),
        )
        .route(
            "/components/:comp_id/schedules",
            get(schedules::list_component_schedules).post(schedules::create_component_schedule),
        )
        .route(
            "/schedules/:id",
            get(schedules::get_schedule)
                .put(schedules::update_schedule)
                .delete(schedules::delete_schedule),
        )
        .route("/schedules/:id/toggle", post(schedules::toggle_schedule))
        .route("/schedules/:id/run-now", post(schedules::run_schedule_now))
        .route(
            "/schedules/:id/executions",
            get(schedules::list_schedule_executions),
        )
        .route("/schedules/presets", get(schedules::list_presets))
        // Air-gap agent update
        .route(
            "/admin/agent-binaries",
            get(agent_update::list_binaries).post(agent_update::upload_binary),
        )
        .route(
            "/admin/agents/:id/update",
            post(agent_update::push_update_to_agent),
        )
        .route(
            "/admin/agents/update-batch",
            post(agent_update::push_update_batch),
        )
        .route(
            "/admin/agent-update-tasks",
            get(agent_update::list_update_tasks),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            state,
            crate::middleware::auth::auth_middleware,
        ))
}
