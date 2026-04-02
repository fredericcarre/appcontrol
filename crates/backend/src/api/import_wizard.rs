//! Enhanced Map Import with Gateway Resolution, Binding Profiles & DR Support
//!
//! This module provides a multi-step import wizard:
//! 1. Upload map file (YAML/JSON)
//! 2. Select gateways (primary + optional DR)
//! 3. Preview host→agent resolution
//! 4. Resolve conflicts (user selects agents for unresolved/multiple)
//! 5. Create application with binding profiles
//!
//! Key principles:
//! - ALL components must be resolved before import can proceed
//! - Map file (Git) contains portable "host" identifiers
//! - Binding profiles (DB) map hosts → agents per environment
//! - One profile is active at a time; switchover = activate another profile

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::dag;
use crate::core::resolution::{
    list_available_agents, resolve_dr_agent, resolve_host_with_options, AvailableAgent,
    ResolutionResult,
};
use crate::db::UuidArray;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

// ══════════════════════════════════════════════════════════════════════
// Import Preview
// ══════════════════════════════════════════════════════════════════════

/// Request to preview import resolution
#[derive(Debug, Deserialize)]
pub struct ImportPreviewRequest {
    /// JSON or YAML content as string
    pub content: String,
    /// Format: "json" or "yaml"
    pub format: String,
    /// Gateway UUIDs to scope resolution
    pub gateway_ids: Vec<Uuid>,
    /// Optional: DR gateway UUIDs for DR profile suggestions
    pub dr_gateway_ids: Option<Vec<Uuid>>,
}

/// Response from import preview
#[derive(Debug, Serialize)]
pub struct ImportPreviewResponse {
    /// Whether the content parsed successfully
    pub valid: bool,
    /// Application name from the map
    pub application_name: String,
    /// Total number of components
    pub component_count: usize,
    /// Whether all components have been resolved
    pub all_resolved: bool,
    /// Resolution status per component
    pub components: Vec<ComponentResolution>,
    /// All available agents for manual selection (primary site)
    pub available_agents: Vec<AvailableAgent>,
    /// Available agents on DR site (if dr_gateway_ids provided)
    pub dr_available_agents: Option<Vec<AvailableAgent>>,
    /// DR suggestions (if dr_gateway_ids provided)
    pub dr_suggestions: Option<Vec<DrSuggestion>>,
    /// Warnings during parsing
    pub warnings: Vec<String>,
    /// Existing application with same name (conflict detection)
    pub existing_application: Option<ExistingApplicationInfo>,
}

/// Info about existing application for conflict detection
#[derive(Debug, Serialize)]
pub struct ExistingApplicationInfo {
    pub id: Uuid,
    pub name: String,
    pub component_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Resolution status for a single component
#[derive(Debug, Serialize)]
pub struct ComponentResolution {
    /// Component name from map
    pub name: String,
    /// Host from map (FQDN or IP)
    pub host: Option<String>,
    /// Component type
    pub component_type: String,
    /// Resolution result
    pub resolution: ComponentResolutionStatus,
}

/// Status of host resolution for a component
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComponentResolutionStatus {
    /// Resolved to exactly one agent
    Resolved {
        agent_id: Uuid,
        agent_hostname: String,
        gateway_id: Option<Uuid>,
        gateway_name: Option<String>,
        resolved_via: String,
    },
    /// Multiple agents matched - user must choose
    Multiple { candidates: Vec<AgentCandidateDto> },
    /// No agent matched - user must select manually
    Unresolved,
    /// No host specified in map - user must provide
    NoHost,
}

/// Agent candidate for UI selection
#[derive(Debug, Serialize)]
pub struct AgentCandidateDto {
    pub agent_id: Uuid,
    pub hostname: String,
    pub gateway_id: Option<Uuid>,
    pub gateway_name: Option<String>,
    pub ip_addresses: Vec<String>,
    pub matched_via: String,
}

/// DR suggestion for a component
#[derive(Debug, Serialize)]
pub struct DrSuggestion {
    pub component_name: String,
    pub primary_host: String,
    pub suggested_dr_host: Option<String>,
    pub dr_resolution: Option<ComponentResolutionStatus>,
}

/// POST /api/v1/import/preview
///
/// Preview import resolution without creating anything.
/// Returns resolution status for all components.
pub async fn preview_import(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ImportPreviewRequest>,
) -> Result<(StatusCode, Json<ImportPreviewResponse>), ApiError> {
    tracing::debug!(
        content_len = body.content.len(),
        format = %body.format,
        gateway_count = body.gateway_ids.len(),
        "Import preview request received"
    );

    // Parse content based on format
    let import_data = parse_import_content(&body.content, &body.format)?;

    let app_name = import_data
        .application
        .name
        .clone()
        .unwrap_or_else(|| "Unnamed Application".to_string());
    let mut warnings = Vec::new();

    // Check for existing application with same name
    tracing::warn!(app_name = %app_name, org_id = %user.organization_id, "CHECKING FOR EXISTING APPLICATION");

    let existing_row: Option<(Uuid, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, name, created_at FROM applications WHERE organization_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&app_name)
    .fetch_optional(&state.db)
    .await?;

    let existing_application = existing_row.map(|(id, name, created_at)| ExistingApplicationInfo {
        id,
        name,
        component_count: 0, // Simplified - don't need count for now
        created_at,
    });

    tracing::warn!(existing = ?existing_application, "EXISTING APPLICATION CHECK RESULT");

    if existing_application.is_some() {
        warnings.push(format!(
            "An application named '{}' already exists. You can rename or update the existing one.",
            app_name
        ));
    }

    // List all available agents on selected gateways
    let available_agents =
        list_available_agents(&state.db, &body.gateway_ids, user.organization_id)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to list agents: {}", e)))?;

    // Resolve each component
    let mut components = Vec::new();
    let mut all_resolved = true;

    for comp in &import_data.application.components {
        let comp_name = comp.name.clone().unwrap_or_else(|| "unnamed".to_string());
        let comp_type = comp
            .component_type
            .clone()
            .unwrap_or_else(|| "service".to_string());

        let resolution = if let Some(ref host) = comp.host {
            let result =
                resolve_host_with_options(&state.db, host, &body.gateway_ids, user.organization_id)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Resolution failed: {}", e)))?;

            match result {
                ResolutionResult::Resolved {
                    agent_id,
                    agent_hostname,
                    gateway_id,
                    gateway_name,
                    resolved_via,
                } => ComponentResolutionStatus::Resolved {
                    agent_id,
                    agent_hostname,
                    gateway_id: gateway_id.map(|g| g.into_inner()),
                    gateway_name,
                    resolved_via: resolved_via.to_string(),
                },
                ResolutionResult::Multiple { candidates } => {
                    all_resolved = false;
                    ComponentResolutionStatus::Multiple {
                        candidates: candidates
                            .into_iter()
                            .map(|c| AgentCandidateDto {
                                agent_id: *c.agent_id,
                                hostname: c.hostname,
                                gateway_id: c.gateway_id.map(|g| *g),
                                gateway_name: c.gateway_name,
                                ip_addresses: c.ip_addresses,
                                matched_via: c.matched_via.to_string(),
                            })
                            .collect(),
                    }
                }
                ResolutionResult::Unresolved => {
                    all_resolved = false;
                    warnings.push(format!(
                        "Component '{}' host '{}' could not be resolved",
                        comp_name, host
                    ));
                    ComponentResolutionStatus::Unresolved
                }
            }
        } else {
            all_resolved = false;
            warnings.push(format!("Component '{}' has no host specified", comp_name));
            ComponentResolutionStatus::NoHost
        };

        components.push(ComponentResolution {
            name: comp_name,
            host: comp.host.clone(),
            component_type: comp_type,
            resolution,
        });
    }

    // Generate DR suggestions if DR gateways specified
    let dr_suggestions = if let Some(ref dr_gw_ids) = body.dr_gateway_ids {
        if !dr_gw_ids.is_empty() {
            let mut suggestions = Vec::new();
            for comp in &import_data.application.components {
                if let Some(ref host) = comp.host {
                    let dr_result =
                        resolve_dr_agent(&state.db, user.organization_id, dr_gw_ids, host)
                            .await
                            .map_err(|e| {
                                ApiError::Internal(format!("DR resolution failed: {}", e))
                            })?;

                    let (suggested_host, dr_resolution) = match dr_result {
                        Some((suggested, result)) => {
                            let status = match result {
                                ResolutionResult::Resolved {
                                    agent_id,
                                    agent_hostname,
                                    gateway_id,
                                    gateway_name,
                                    resolved_via,
                                } => Some(ComponentResolutionStatus::Resolved {
                                    agent_id,
                                    agent_hostname,
                                    gateway_id: gateway_id.map(|g| g.into_inner()),
                                    gateway_name,
                                    resolved_via: resolved_via.to_string(),
                                }),
                                ResolutionResult::Multiple { candidates } => {
                                    Some(ComponentResolutionStatus::Multiple {
                                        candidates: candidates
                                            .into_iter()
                                            .map(|c| AgentCandidateDto {
                                                agent_id: *c.agent_id,
                                                hostname: c.hostname,
                                                gateway_id: c.gateway_id.map(|g| *g),
                                                gateway_name: c.gateway_name,
                                                ip_addresses: c.ip_addresses,
                                                matched_via: c.matched_via.to_string(),
                                            })
                                            .collect(),
                                    })
                                }
                                ResolutionResult::Unresolved => {
                                    Some(ComponentResolutionStatus::Unresolved)
                                }
                            };
                            (Some(suggested), status)
                        }
                        None => (None, None),
                    };

                    suggestions.push(DrSuggestion {
                        component_name: comp.name.clone().unwrap_or_default(),
                        primary_host: host.clone(),
                        suggested_dr_host: suggested_host,
                        dr_resolution,
                    });
                }
            }
            Some(suggestions)
        } else {
            None
        }
    } else {
        None
    };

    // List DR available agents if DR gateways specified
    let dr_available_agents = if let Some(ref dr_gw_ids) = body.dr_gateway_ids {
        if !dr_gw_ids.is_empty() {
            Some(
                list_available_agents(&state.db, dr_gw_ids, user.organization_id)
                    .await
                    .map_err(|e| ApiError::Internal(format!("Failed to list DR agents: {}", e)))?,
            )
        } else {
            None
        }
    } else {
        None
    };

    let response = ImportPreviewResponse {
        valid: true,
        application_name: app_name,
        component_count: components.len(),
        all_resolved,
        components,
        available_agents,
        dr_available_agents,
        dr_suggestions,
        warnings,
        existing_application,
    };

    Ok((StatusCode::OK, Json(response)))
}

// ══════════════════════════════════════════════════════════════════════
// Import Execute
// ══════════════════════════════════════════════════════════════════════

/// How to handle name conflicts
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
    /// Fail if application exists (default)
    #[default]
    Fail,
    /// Rename: use the new_name field
    Rename,
    /// Update existing application (replace components and profiles)
    Update,
}

/// Request to execute import with binding profiles
#[derive(Debug, Deserialize)]
pub struct ImportExecuteRequest {
    /// JSON or YAML content as string
    pub content: String,
    /// Format: "json" or "yaml"
    pub format: String,
    /// Site ID to create application in (optional - auto-selects default site if not provided)
    pub site_id: Option<Uuid>,
    /// Primary binding profile
    pub profile: ProfileConfig,
    /// Optional DR binding profile
    pub dr_profile: Option<ProfileConfig>,
    /// How to handle name conflicts (default: fail)
    #[serde(default)]
    pub conflict_action: ConflictAction,
    /// New name if conflict_action is "rename"
    pub new_name: Option<String>,
}

/// Configuration for a binding profile
#[derive(Debug, Deserialize)]
pub struct ProfileConfig {
    /// Profile name (e.g., "prod", "dr", "bench")
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Profile type: "primary", "dr", or "custom"
    pub profile_type: String,
    /// Gateway UUIDs for this profile
    pub gateway_ids: Vec<Uuid>,
    /// Enable auto-failover (for DR profiles)
    pub auto_failover: Option<bool>,
    /// Component→agent mappings
    pub mappings: Vec<MappingConfig>,
}

/// A single component→agent mapping
#[derive(Debug, Deserialize)]
pub struct MappingConfig {
    /// Component name (from map)
    pub component_name: String,
    /// Resolved agent ID
    pub agent_id: Uuid,
    /// How it was resolved
    pub resolved_via: String,
}

/// Response from import execute
#[derive(Debug, Serialize)]
pub struct ImportExecuteResponse {
    /// Created application ID
    pub application_id: Uuid,
    /// Application name
    pub application_name: String,
    /// Number of components created
    pub components_created: usize,
    /// Profiles created
    pub profiles_created: Vec<String>,
    /// Active profile name
    pub active_profile: String,
    /// Warnings
    pub warnings: Vec<String>,
}

/// POST /api/v1/import/execute
///
/// Execute import with binding profiles.
/// ALL components must have an agent mapping.
pub async fn execute_import(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ImportExecuteRequest>,
) -> Result<(StatusCode, Json<ImportExecuteResponse>), ApiError> {
    // Parse content
    let import_data = parse_import_content(&body.content, &body.format)?;

    // Resolve site_id: use provided value or auto-select default site
    let site_id = match body.site_id {
        Some(id) => id,
        None => {
            // Find default site for organization (prefer 'primary' type)
            #[cfg(feature = "postgres")]
            let site: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM sites WHERE organization_id = $1 AND is_active = true ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
            )
            .bind(crate::db::bind_id(user.organization_id))
            .fetch_optional(&state.db)
            .await?;

            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let site: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM sites WHERE organization_id = $1 AND is_active = 1 ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
            )
            .bind(crate::db::bind_id(user.organization_id))
            .fetch_optional(&state.db)
            .await?;

            match site {
                Some((id,)) => id,
                None => {
                    // Create a default site if none exists
                    let new_site_id = Uuid::new_v4();
                    sqlx::query(
                        "INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, $3, $4, $5)",
                    )
                    .bind(new_site_id)
                    .bind(crate::db::bind_id(user.organization_id))
                    .bind("Default Site")
                    .bind("DEFAULT")
                    .bind("primary")
                    .execute(&state.db)
                    .await?;
                    new_site_id
                }
            }
        }
    };

    let original_name = import_data
        .application
        .name
        .clone()
        .unwrap_or_else(|| "Imported Application".to_string());

    // Check for existing application and handle conflicts
    let existing_app: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM applications WHERE organization_id = $1 AND name = $2")
            .bind(crate::db::bind_id(user.organization_id))
            .bind(&original_name)
            .fetch_optional(&state.db)
            .await?;

    let (app_id, app_name, is_update) = match (&body.conflict_action, existing_app) {
        (_, None) => {
            // No conflict - create new
            (Uuid::new_v4(), original_name, false)
        }
        (ConflictAction::Fail, Some(_)) => {
            return Err(ApiError::Conflict(format!(
                "Application '{}' already exists. Use conflict_action 'rename' or 'update' to proceed.",
                original_name
            )));
        }
        (ConflictAction::Rename, Some(_)) => {
            let new_name = body.new_name.clone().ok_or_else(|| {
                ApiError::Validation(
                    "new_name is required when conflict_action is 'rename'".to_string(),
                )
            })?;
            // Check new name doesn't exist either
            let new_exists: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM applications WHERE organization_id = $1 AND name = $2",
            )
            .bind(crate::db::bind_id(user.organization_id))
            .bind(&new_name)
            .fetch_optional(&state.db)
            .await?;
            if new_exists.is_some() {
                return Err(ApiError::Conflict(format!(
                    "Application '{}' also already exists. Choose a different name.",
                    new_name
                )));
            }
            (Uuid::new_v4(), new_name, false)
        }
        (ConflictAction::Update, Some((existing_id,))) => {
            // Delete existing components and profiles, keep application
            sqlx::query("DELETE FROM components WHERE application_id = $1")
                .bind(existing_id)
                .execute(&state.db)
                .await?;
            sqlx::query("DELETE FROM binding_profiles WHERE application_id = $1")
                .bind(existing_id)
                .execute(&state.db)
                .await?;
            sqlx::query("DELETE FROM app_variables WHERE application_id = $1")
                .bind(existing_id)
                .execute(&state.db)
                .await?;
            sqlx::query("DELETE FROM component_groups WHERE application_id = $1")
                .bind(existing_id)
                .execute(&state.db)
                .await?;
            (existing_id, original_name, true)
        }
    };

    // Validate all components have mappings
    let component_names: Vec<_> = import_data
        .application
        .components
        .iter()
        .filter_map(|c| c.name.clone())
        .collect();

    let mapped_names: Vec<_> = body
        .profile
        .mappings
        .iter()
        .map(|m| &m.component_name)
        .collect();

    for name in &component_names {
        if !mapped_names.contains(&name) {
            return Err(ApiError::Validation(format!(
                "Component '{}' has no agent mapping. All components must be resolved.",
                name
            )));
        }
    }

    // Build mappings lookup
    let mappings_map: HashMap<_, _> = body
        .profile
        .mappings
        .iter()
        .map(|m| (m.component_name.clone(), m))
        .collect();

    let mut warnings = Vec::new();

    // Log import action BEFORE creating
    let action_type = if is_update {
        "update_with_profiles"
    } else {
        "import_with_profiles"
    };
    log_action(
        &state.db,
        user.user_id,
        action_type,
        "application",
        app_id,
        json!({
            "name": &app_name,
            "profile": &body.profile.name,
            "dr_profile": body.dr_profile.as_ref().map(|p| &p.name),
            "is_update": is_update
        }),
    )
    .await?;

    // Create or update application
    let tags_json = serde_json::to_value(&import_data.application.tags).unwrap_or(Value::Null);
    if is_update {
        sqlx::query(
            &format!("UPDATE applications SET description = $1, site_id = $2, tags = $3, updated_at = {} WHERE id = $4", crate::db::sql::now()),
        )
        .bind(&import_data.application.description)
        .bind(crate::db::bind_id(site_id))
        .bind(&tags_json)
        .bind(crate::db::bind_id(app_id))
        .execute(&state.db)
        .await?;
        warnings.push("Existing application updated with new components and profiles.".to_string());
    } else {
        sqlx::query(
            "INSERT INTO applications (id, name, description, organization_id, site_id, tags) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(&app_name)
        .bind(&import_data.application.description)
        .bind(crate::db::bind_id(user.organization_id))
        .bind(crate::db::bind_id(site_id))
        .bind(&tags_json)
        .execute(&state.db)
        .await?;
    }

    // Grant owner to importing user
    let _ = sqlx::query(
        "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) VALUES ($1, $2, 'owner', $2)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(user.user_id))
    .execute(&state.db)
    .await;

    // Import variables
    for var in &import_data.application.variables {
        sqlx::query(
            "INSERT INTO app_variables (application_id, name, value, description, is_secret) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(&var.name)
        .bind(&var.value)
        .bind(&var.description)
        .bind(var.is_secret)
        .execute(&state.db)
        .await?;
    }

    // Import groups
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    for (idx, group) in import_data.application.groups.iter().enumerate() {
        let group_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO component_groups (id, application_id, name, description, color, display_order) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(group_id)
        .bind(crate::db::bind_id(app_id))
        .bind(&group.name)
        .bind(&group.description)
        .bind(&group.color)
        .bind(idx as i32)
        .execute(&state.db)
        .await?;

        group_map.insert(group.name.clone(), group_id);
    }

    // Import components
    let mut comp_name_to_id: HashMap<String, Uuid> = HashMap::new();
    let mut components_created = 0;

    for (idx, comp) in import_data.application.components.iter().enumerate() {
        let comp_name = comp
            .name
            .clone()
            .unwrap_or_else(|| format!("component_{}", idx));
        let comp_id = Uuid::new_v4();

        // Get agent_id from mapping
        let agent_id = mappings_map
            .get(&comp_name)
            .map(|m| m.agent_id)
            .ok_or_else(|| {
                ApiError::Validation(format!("No mapping for component '{}'", comp_name))
            })?;

        let group_id = comp.group.as_ref().and_then(|g| group_map.get(g)).copied();

        let comp_type = comp.component_type.as_deref().unwrap_or("service");
        let icon = comp
            .icon
            .as_deref()
            .unwrap_or(default_icon_for_type(comp_type));

        // Extract commands (flat fields take precedence over nested commands object)
        let check_cmd = comp
            .check_cmd
            .as_ref()
            .or_else(|| comp.commands.check.as_ref().map(|c| &c.cmd));
        let start_cmd = comp
            .start_cmd
            .as_ref()
            .or_else(|| comp.commands.start.as_ref().map(|c| &c.cmd));
        let stop_cmd = comp
            .stop_cmd
            .as_ref()
            .or_else(|| comp.commands.stop.as_ref().map(|c| &c.cmd));
        let integrity_cmd = comp
            .integrity_check_cmd
            .as_ref()
            .or_else(|| comp.commands.integrity_check.as_ref().map(|c| &c.cmd));
        let post_start_cmd = comp.commands.post_start_check.as_ref().map(|c| &c.cmd);
        let infra_cmd = comp
            .infra_check_cmd
            .as_ref()
            .or_else(|| comp.commands.infra_check.as_ref().map(|c| &c.cmd));
        let rebuild_cmd = comp
            .rebuild_cmd
            .as_ref()
            .or_else(|| comp.commands.rebuild.as_ref().map(|c| &c.cmd));
        let rebuild_infra_cmd = comp.commands.rebuild_infra.as_ref().map(|c| &c.cmd);

        // Extract position (position object takes precedence over position_x/y)
        let pos_x = comp
            .position
            .as_ref()
            .map(|p| p.x)
            .or(comp.position_x)
            .unwrap_or((idx % 5) as f32 * 250.0);
        let pos_y = comp
            .position
            .as_ref()
            .map(|p| p.y)
            .or(comp.position_y)
            .unwrap_or((idx / 5) as f32 * 200.0);

        // Convert cluster_nodes to JSONB
        let cluster_nodes_json: Option<serde_json::Value> = comp
            .cluster_nodes
            .as_ref()
            .map(|nodes| serde_json::json!(nodes));

        sqlx::query(
            r#"INSERT INTO components (
                id, application_id, name, display_name, description, component_type,
                icon, group_id, host, agent_id, check_cmd, start_cmd, stop_cmd,
                integrity_check_cmd, post_start_check_cmd, infra_check_cmd,
                rebuild_cmd, rebuild_infra_cmd,
                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds,
                is_optional, position_x, position_y, cluster_size, cluster_nodes
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26
            )"#,
        )
        .bind(crate::db::bind_id(comp_id))
        .bind(crate::db::bind_id(app_id))
        .bind(&comp_name)
        .bind(&comp.display_name)
        .bind(&comp.description)
        .bind(comp_type)
        .bind(icon)
        .bind(group_id)
        .bind(&comp.host)
        .bind(crate::db::bind_id(agent_id))
        .bind(check_cmd)
        .bind(start_cmd)
        .bind(stop_cmd)
        .bind(integrity_cmd)
        .bind(post_start_cmd)
        .bind(infra_cmd)
        .bind(rebuild_cmd)
        .bind(rebuild_infra_cmd)
        .bind(comp.check_interval_seconds)
        .bind(comp.start_timeout_seconds)
        .bind(comp.stop_timeout_seconds)
        .bind(comp.is_optional)
        .bind(pos_x)
        .bind(pos_y)
        .bind(comp.cluster_size)
        .bind(&cluster_nodes_json)
        .execute(&state.db)
        .await?;

        comp_name_to_id.insert(comp_name.clone(), comp_id);
        components_created += 1;

        // Import custom commands
        for custom_cmd in &comp.custom_commands {
            let cmd_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation)
                VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(cmd_id)
            .bind(crate::db::bind_id(comp_id))
            .bind(&custom_cmd.name)
            .bind(&custom_cmd.command)
            .bind(&custom_cmd.description)
            .bind(custom_cmd.requires_confirmation)
            .execute(&state.db)
            .await?;

            // Import parameters
            for (pidx, param) in custom_cmd.parameters.iter().enumerate() {
                let enum_vals_json = param
                    .enum_values
                    .as_ref()
                    .and_then(|v| serde_json::to_value(v).ok());

                sqlx::query(
                    r#"INSERT INTO command_input_params (
                        command_id, name, description, default_value, validation_regex,
                        required, param_type, enum_values, display_order
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
                )
                .bind(cmd_id)
                .bind(&param.name)
                .bind(&param.description)
                .bind(&param.default_value)
                .bind(&param.validation_regex)
                .bind(param.required)
                .bind(&param.param_type)
                .bind(&enum_vals_json)
                .bind(pidx as i32)
                .execute(&state.db)
                .await?;
            }
        }

        // Import links
        for (lidx, link) in comp.links.iter().enumerate() {
            sqlx::query(
                "INSERT INTO component_links (component_id, label, url, link_type, display_order) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(crate::db::bind_id(comp_id))
            .bind(&link.label)
            .bind(&link.url)
            .bind(&link.link_type)
            .bind(lidx as i32)
            .execute(&state.db)
            .await?;
        }

        // Import site overrides for this component
        for override_data in &comp.site_overrides {
            // Look up site_id by site_code
            let site_row: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM sites WHERE organization_id = $1 AND code = $2")
                    .bind(crate::db::bind_id(user.organization_id))
                    .bind(&override_data.site_code)
                    .fetch_optional(&state.db)
                    .await?;

            let override_site_id = match site_row {
                Some((id,)) => id,
                None => {
                    warnings.push(format!(
                        "Component '{}': site override for code '{}' skipped - site not found",
                        comp_name, override_data.site_code
                    ));
                    continue;
                }
            };

            // Resolve agent_id from host_override if provided
            let agent_id_override: Option<Uuid> = if let Some(ref host) =
                override_data.host_override
            {
                // Look up agent by hostname or IP at this site's gateway
                #[cfg(feature = "postgres")]
                let agent_row: Option<(Uuid,)> = sqlx::query_as(
                    r#"SELECT a.id FROM agents a
                       JOIN gateways g ON a.gateway_id = g.id
                       WHERE a.organization_id = $1
                         AND g.site_id = $2
                         AND (a.hostname ILIKE $3 OR EXISTS (
                           SELECT 1 FROM jsonb_array_elements_text(a.ip_addresses) ip
                           WHERE ip = $3
                         ))
                       LIMIT 1"#,
                )
                .bind(crate::db::bind_id(user.organization_id))
                .bind(override_site_id)
                .bind(host)
                .fetch_optional(&state.db)
                .await?;

                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let agent_row: Option<(Uuid,)> = sqlx::query_as(
                    r#"SELECT a.id FROM agents a
                       JOIN gateways g ON a.gateway_id = g.id
                       WHERE a.organization_id = $1
                         AND g.site_id = $2
                         AND (a.hostname LIKE $3 OR EXISTS (
                           SELECT 1 FROM json_each(a.ip_addresses)
                           WHERE value = $3
                         ))
                       LIMIT 1"#,
                )
                .bind(crate::db::bind_id(user.organization_id))
                .bind(override_site_id)
                .bind(host)
                .fetch_optional(&state.db)
                .await?;

                match agent_row {
                    Some((id,)) => Some(id),
                    None => {
                        warnings.push(format!(
                            "Component '{}': site '{}' host_override '{}' could not be resolved to an agent",
                            comp_name, override_data.site_code, host
                        ));
                        None
                    }
                }
            } else {
                None
            };

            // Insert site override
            sqlx::query(
                r#"INSERT INTO site_overrides (component_id, site_id, agent_id_override,
                    check_cmd_override, start_cmd_override, stop_cmd_override,
                    rebuild_cmd_override, env_vars_override)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (component_id, site_id) DO UPDATE SET
                    agent_id_override = EXCLUDED.agent_id_override,
                    check_cmd_override = EXCLUDED.check_cmd_override,
                    start_cmd_override = EXCLUDED.start_cmd_override,
                    stop_cmd_override = EXCLUDED.stop_cmd_override,
                    rebuild_cmd_override = EXCLUDED.rebuild_cmd_override,
                    env_vars_override = EXCLUDED.env_vars_override"#,
            )
            .bind(crate::db::bind_id(comp_id))
            .bind(override_site_id)
            .bind(agent_id_override)
            .bind(&override_data.check_cmd_override)
            .bind(&override_data.start_cmd_override)
            .bind(&override_data.stop_cmd_override)
            .bind(&override_data.rebuild_cmd_override)
            .bind(&override_data.env_vars_override)
            .execute(&state.db)
            .await?;
        }
    }

    // Import dependencies
    for dep in &import_data.application.dependencies {
        let from_id = match comp_name_to_id.get(&dep.from) {
            Some(id) => *id,
            None => {
                warnings.push(format!(
                    "Dependency from '{}' to '{}': source not found",
                    dep.from, dep.to
                ));
                continue;
            }
        };
        let to_id = match comp_name_to_id.get(&dep.to) {
            Some(id) => *id,
            None => {
                warnings.push(format!(
                    "Dependency from '{}' to '{}': target not found",
                    dep.from, dep.to
                ));
                continue;
            }
        };

        sqlx::query(
            "INSERT INTO dependencies (application_id, from_component_id, to_component_id) VALUES ($1, $2, $3)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(from_id)
        .bind(to_id)
        .execute(&state.db)
        .await?;
    }

    // Validate DAG
    let dag_result = dag::build_dag(&state.db, app_id).await;
    if let Ok(dag) = dag_result {
        if let Err(cycle_err) = dag.topological_levels() {
            warnings.push(format!("Warning: DAG contains a cycle - {}", cycle_err));
        }
    }

    // Create primary binding profile
    let primary_profile_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(primary_profile_id)
    .bind(crate::db::bind_id(app_id))
    .bind(&body.profile.name)
    .bind(&body.profile.description)
    .bind(&body.profile.profile_type)
    .bind(true) // Primary is active by default
    .bind(UuidArray::from(body.profile.gateway_ids.clone()))
    .bind(body.profile.auto_failover.unwrap_or(false))
    .bind(crate::db::bind_id(user.user_id))
    .execute(&state.db)
    .await?;

    // Create mappings for primary profile
    for mapping in &body.profile.mappings {
        let host = import_data
            .application
            .components
            .iter()
            .find(|c| c.name.as_ref() == Some(&mapping.component_name))
            .and_then(|c| c.host.clone())
            .unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via)
            VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(primary_profile_id)
        .bind(&mapping.component_name)
        .bind(&host)
        .bind(mapping.agent_id)
        .bind(&mapping.resolved_via)
        .execute(&state.db)
        .await?;
    }

    let mut profiles_created = vec![body.profile.name.clone()];

    // Create DR profile if specified
    if let Some(ref dr_profile) = body.dr_profile {
        // Note: DR profiles can have partial mappings - components not mapped
        // are intentionally disabled on the DR site (not replicated)

        let dr_profile_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(dr_profile_id)
        .bind(crate::db::bind_id(app_id))
        .bind(&dr_profile.name)
        .bind(&dr_profile.description)
        .bind(&dr_profile.profile_type)
        .bind(false) // DR is inactive by default
        .bind(UuidArray::from(dr_profile.gateway_ids.clone()))
        .bind(dr_profile.auto_failover.unwrap_or(false))
        .bind(crate::db::bind_id(user.user_id))
        .execute(&state.db)
        .await?;

        // Create mappings for DR profile
        for mapping in &dr_profile.mappings {
            let host = import_data
                .application
                .components
                .iter()
                .find(|c| c.name.as_ref() == Some(&mapping.component_name))
                .and_then(|c| c.host.clone())
                .unwrap_or_default();

            sqlx::query(
                r#"INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via)
                VALUES ($1, $2, $3, $4, $5)"#,
            )
            .bind(dr_profile_id)
            .bind(&mapping.component_name)
            .bind(&host)
            .bind(mapping.agent_id)
            .bind(&mapping.resolved_via)
            .execute(&state.db)
            .await?;
        }

        profiles_created.push(dr_profile.name.clone());
    }

    // Push config to affected agents so they start health checks immediately
    crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None).await;

    let response = ImportExecuteResponse {
        application_id: app_id,
        application_name: app_name,
        components_created,
        profiles_created,
        active_profile: body.profile.name.clone(),
        warnings,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

// ══════════════════════════════════════════════════════════════════════
// Internal data structures
// ══════════════════════════════════════════════════════════════════════

/// Parsed import data (common for YAML and JSON)
#[derive(Debug, Deserialize)]
struct ImportData {
    #[allow(dead_code)]
    format_version: Option<String>,
    application: ApplicationData,
}

#[derive(Debug, Deserialize)]
struct ApplicationData {
    name: Option<String>,
    description: Option<String>,
    /// Tags can be either array of strings or object with key-value pairs
    #[serde(default, deserialize_with = "deserialize_tags")]
    tags: Vec<String>,
    #[serde(default)]
    variables: Vec<VariableData>,
    #[serde(default)]
    groups: Vec<GroupData>,
    #[serde(default)]
    components: Vec<ComponentData>,
    #[serde(default)]
    dependencies: Vec<DependencyData>,
    // Extra fields are ignored (dr_config, notes, etc.)
    #[serde(flatten)]
    _extra: Option<serde_json::Value>,
}

/// Deserialize tags from either array or object format
fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    use serde_json::Value;

    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>()
            .pipe(Ok),
        Value::Object(obj) => {
            // Convert {"env": "prod", "tier": "1"} to ["env:prod", "tier:1"]
            Ok(obj
                .into_iter()
                .map(|(k, v)| format!("{}:{}", k, v.as_str().unwrap_or(&v.to_string())))
                .collect())
        }
        Value::Null => Ok(Vec::new()),
        _ => Err(de::Error::custom("tags must be array or object")),
    }
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}

#[derive(Debug, Deserialize)]
struct VariableData {
    name: String,
    value: String,
    description: Option<String>,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Deserialize)]
struct GroupData {
    name: String,
    description: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ComponentData {
    name: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(alias = "type")]
    component_type: Option<String>,
    icon: Option<String>,
    group: Option<String>,
    host: Option<String>,
    /// Commands can be in nested "commands" object or flat on component
    #[serde(default)]
    commands: CommandsData,
    /// Flat command fields (alternative to nested commands object)
    check_cmd: Option<String>,
    start_cmd: Option<String>,
    stop_cmd: Option<String>,
    integrity_check_cmd: Option<String>,
    infra_check_cmd: Option<String>,
    rebuild_cmd: Option<String>,
    #[serde(default)]
    custom_commands: Vec<CustomCommandData>,
    #[serde(default)]
    links: Vec<LinkData>,
    /// Position can be {x, y} object or separate position_x/position_y
    position: Option<PositionData>,
    position_x: Option<f32>,
    position_y: Option<f32>,
    #[serde(default = "default_check_interval", alias = "check_interval_secs")]
    check_interval_seconds: i32,
    #[serde(default = "default_start_timeout", alias = "start_timeout_secs")]
    start_timeout_seconds: i32,
    #[serde(default = "default_stop_timeout", alias = "stop_timeout_secs")]
    stop_timeout_seconds: i32,
    #[serde(default)]
    is_optional: bool,
    #[serde(default, alias = "protected")]
    rebuild_protected: bool,
    /// Cluster size (number of nodes, >= 2 for clusters)
    cluster_size: Option<i32>,
    /// List of cluster node hostnames/IPs
    cluster_nodes: Option<Vec<String>>,
    /// Site-specific overrides for failover (keyed by site code)
    #[serde(default)]
    site_overrides: Vec<SiteOverrideData>,
    // Ignore extra fields
    #[serde(flatten)]
    _extra: Option<serde_json::Value>,
}

/// Site-specific command overrides for DR/failover
#[derive(Debug, Deserialize)]
struct SiteOverrideData {
    /// Site code (e.g., "DR", "BENCH") - matched to sites.code
    site_code: String,
    /// Override host for this site (mapped to agent during resolution)
    host_override: Option<String>,
    /// Override check command for this site
    check_cmd_override: Option<String>,
    /// Override start command for this site
    start_cmd_override: Option<String>,
    /// Override stop command for this site
    stop_cmd_override: Option<String>,
    /// Override rebuild command for this site
    rebuild_cmd_override: Option<String>,
    /// Override environment variables for this site (JSON object)
    env_vars_override: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PositionData {
    x: f32,
    y: f32,
}

fn default_check_interval() -> i32 {
    30
}
fn default_start_timeout() -> i32 {
    300
}
fn default_stop_timeout() -> i32 {
    120
}

#[derive(Debug, Deserialize, Default)]
struct CommandsData {
    check: Option<CommandDetail>,
    start: Option<CommandDetail>,
    stop: Option<CommandDetail>,
    integrity_check: Option<CommandDetail>,
    post_start_check: Option<CommandDetail>,
    infra_check: Option<CommandDetail>,
    rebuild: Option<CommandDetail>,
    rebuild_infra: Option<CommandDetail>,
}

#[derive(Debug, Deserialize)]
struct CommandDetail {
    cmd: String,
    #[allow(dead_code)]
    timeout_seconds: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct CustomCommandData {
    name: String,
    command: String,
    description: Option<String>,
    #[serde(default)]
    requires_confirmation: bool,
    #[serde(default)]
    parameters: Vec<CommandParamData>,
}

#[derive(Debug, Deserialize)]
struct CommandParamData {
    name: String,
    description: Option<String>,
    default_value: Option<String>,
    validation_regex: Option<String>,
    #[serde(default = "default_true")]
    required: bool,
    #[serde(default = "default_param_type")]
    param_type: String,
    enum_values: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}
fn default_param_type() -> String {
    "string".to_string()
}

#[derive(Debug, Deserialize)]
struct LinkData {
    label: String,
    url: String,
    #[serde(default = "default_link_type")]
    link_type: String,
}

fn default_link_type() -> String {
    "other".to_string()
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DependencyData {
    from: String,
    to: String,
    #[serde(alias = "type")]
    dep_type: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════
// Helper functions
// ══════════════════════════════════════════════════════════════════════

fn parse_import_content(content: &str, format: &str) -> Result<ImportData, ApiError> {
    match format.to_lowercase().as_str() {
        "json" => {
            // Try parsing with "application" wrapper first
            if let Ok(data) = serde_json::from_str::<ImportData>(content) {
                return Ok(data);
            }
            // Try parsing without wrapper (direct ApplicationData)
            serde_json::from_str::<ApplicationData>(content)
                .map(|app| ImportData {
                    format_version: None,
                    application: app,
                })
                .map_err(|e| {
                    tracing::warn!("JSON parse error: {}", e);
                    ApiError::Validation(format!("Invalid JSON: {}", e))
                })
        }
        "yaml" | "yml" => {
            // Try parsing with "application" wrapper first
            if let Ok(data) = serde_yaml::from_str::<ImportData>(content) {
                return Ok(data);
            }
            // Try parsing without wrapper (direct ApplicationData)
            serde_yaml::from_str::<ApplicationData>(content)
                .map(|app| ImportData {
                    format_version: None,
                    application: app,
                })
                .map_err(|e| {
                    tracing::warn!("YAML parse error: {}", e);
                    ApiError::Validation(format!("Invalid YAML: {}", e))
                })
        }
        _ => Err(ApiError::Validation(format!(
            "Unsupported format '{}'. Use 'json' or 'yaml'",
            format
        ))),
    }
}

/// Get default icon based on component type
fn default_icon_for_type(comp_type: &str) -> &'static str {
    match comp_type.to_lowercase().as_str() {
        "database" | "db" => "database",
        "middleware" | "mq" | "queue" | "messaging" | "layers" => "layers",
        "appserver" | "app" | "application" | "server" => "server",
        "webfront" | "web" | "webserver" | "frontend" => "globe",
        "service" | "svc" | "api" => "cog",
        "batch" | "job" | "scheduler" => "clock",
        "loadbalancer" | "lb" | "proxy" | "gateway" => "network",
        "cache" | "redis" | "memcached" => "zap",
        _ => "box",
    }
}
