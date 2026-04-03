//! Import preview handler.

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::resolution::{
    list_available_agents, resolve_dr_agent, resolve_host_with_options,
    ResolutionResult,
};
use crate::error::ApiError;
use crate::AppState;

use super::types::*;

/// POST /api/v1/import/preview
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

    let import_data = parse_import_content(&body.content, &body.format)?;
    let app_name = import_data.application.name.clone()
        .unwrap_or_else(|| "Unnamed Application".to_string());
    let mut warnings = Vec::new();

    // Check for existing application with same name
    let existing_row: Option<(Uuid, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, name, created_at FROM applications WHERE organization_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&app_name)
    .fetch_optional(&state.db)
    .await?;

    let existing_application = existing_row.map(|(id, name, created_at)| ExistingApplicationInfo {
        id, name, component_count: 0, created_at,
    });

    if existing_application.is_some() {
        warnings.push(format!("An application named '{}' already exists. You can rename or update the existing one.", app_name));
    }

    let available_agents = list_available_agents(&state.db, &body.gateway_ids, user.organization_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to list agents: {}", e)))?;

    // Resolve each component
    let mut components = Vec::new();
    let mut all_resolved = true;

    for comp in &import_data.application.components {
        let comp_name = comp.name.clone().unwrap_or_else(|| "unnamed".to_string());
        let comp_type = comp.component_type.clone().unwrap_or_else(|| "service".to_string());

        let resolution = if let Some(ref host) = comp.host {
            let result = resolve_host_with_options(&state.db, host, &body.gateway_ids, user.organization_id)
                .await
                .map_err(|e| ApiError::Internal(format!("Resolution failed: {}", e)))?;

            match result {
                ResolutionResult::Resolved { agent_id, agent_hostname, gateway_id, gateway_name, resolved_via } =>
                    ComponentResolutionStatus::Resolved {
                        agent_id, agent_hostname,
                        gateway_id: gateway_id.map(|g| g.into_inner()),
                        gateway_name, resolved_via: resolved_via.to_string(),
                    },
                ResolutionResult::Multiple { candidates } => {
                    all_resolved = false;
                    ComponentResolutionStatus::Multiple {
                        candidates: candidates.into_iter().map(|c| AgentCandidateDto {
                            agent_id: *c.agent_id, hostname: c.hostname,
                            gateway_id: c.gateway_id.map(|g| *g), gateway_name: c.gateway_name,
                            ip_addresses: c.ip_addresses, matched_via: c.matched_via.to_string(),
                        }).collect(),
                    }
                }
                ResolutionResult::Unresolved => {
                    all_resolved = false;
                    warnings.push(format!("Component '{}' host '{}' could not be resolved", comp_name, host));
                    ComponentResolutionStatus::Unresolved
                }
            }
        } else {
            all_resolved = false;
            warnings.push(format!("Component '{}' has no host specified", comp_name));
            ComponentResolutionStatus::NoHost
        };

        components.push(ComponentResolution { name: comp_name, host: comp.host.clone(), component_type: comp_type, resolution });
    }

    // Generate DR suggestions if DR gateways specified
    let dr_suggestions = if let Some(ref dr_gw_ids) = body.dr_gateway_ids {
        if !dr_gw_ids.is_empty() {
            let mut suggestions = Vec::new();
            for comp in &import_data.application.components {
                if let Some(ref host) = comp.host {
                    let dr_result = resolve_dr_agent(&state.db, user.organization_id, dr_gw_ids, host)
                        .await.map_err(|e| ApiError::Internal(format!("DR resolution failed: {}", e)))?;

                    let (suggested_host, dr_resolution) = match dr_result {
                        Some((suggested, result)) => {
                            let status = match result {
                                ResolutionResult::Resolved { agent_id, agent_hostname, gateway_id, gateway_name, resolved_via } =>
                                    Some(ComponentResolutionStatus::Resolved {
                                        agent_id, agent_hostname,
                                        gateway_id: gateway_id.map(|g| g.into_inner()),
                                        gateway_name, resolved_via: resolved_via.to_string(),
                                    }),
                                ResolutionResult::Multiple { candidates } =>
                                    Some(ComponentResolutionStatus::Multiple {
                                        candidates: candidates.into_iter().map(|c| AgentCandidateDto {
                                            agent_id: *c.agent_id, hostname: c.hostname,
                                            gateway_id: c.gateway_id.map(|g| *g), gateway_name: c.gateway_name,
                                            ip_addresses: c.ip_addresses, matched_via: c.matched_via.to_string(),
                                        }).collect(),
                                    }),
                                ResolutionResult::Unresolved => Some(ComponentResolutionStatus::Unresolved),
                            };
                            (Some(suggested), status)
                        }
                        None => (None, None),
                    };

                    suggestions.push(DrSuggestion {
                        component_name: comp.name.clone().unwrap_or_default(),
                        primary_host: host.clone(), suggested_dr_host: suggested_host, dr_resolution,
                    });
                }
            }
            Some(suggestions)
        } else { None }
    } else { None };

    let dr_available_agents = if let Some(ref dr_gw_ids) = body.dr_gateway_ids {
        if !dr_gw_ids.is_empty() {
            Some(list_available_agents(&state.db, dr_gw_ids, user.organization_id)
                .await.map_err(|e| ApiError::Internal(format!("Failed to list DR agents: {}", e)))?)
        } else { None }
    } else { None };

    let response = ImportPreviewResponse {
        valid: true, application_name: app_name, component_count: components.len(),
        all_resolved, components, available_agents, dr_available_agents,
        dr_suggestions, warnings, existing_application,
    };

    Ok((StatusCode::OK, Json(response)))
}
