//! JSON Export: Export application configuration in v4 native JSON format.
//!
//! Exports the complete application structure for backup/restore or sharing:
//! - Application metadata (name, description, tags)
//! - Component groups (with colors and ordering)
//! - Components (with commands, positions, types)
//! - Dependencies (referencing components by name for portability)
//! - Variables (optionally including secrets)
//! - Custom commands with input parameters
//! - Component links (hypertext resources)

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::AppState;
use appcontrol_common::PermissionLevel;

// ── Export Query Parameters ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// Include secret variable values (default: false)
    pub include_secrets: Option<bool>,
}

// ── Export Format Structures ────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ExportedApplication {
    pub format_version: &'static str,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub application: ApplicationExport,
}

#[derive(Debug, Serialize)]
pub struct ApplicationExport {
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub variables: Vec<VariableExport>,
    pub groups: Vec<GroupExport>,
    pub components: Vec<ComponentExport>,
    pub dependencies: Vec<DependencyExport>,
}

#[derive(Debug, Serialize)]
pub struct VariableExport {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub is_secret: bool,
}

#[derive(Debug, Serialize)]
pub struct GroupExport {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: i32,
}

#[derive(Debug, Serialize)]
pub struct ComponentExport {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub component_type: String,
    pub icon: Option<String>,
    pub group: Option<String>,
    pub host: Option<String>,
    pub commands: CommandsExport,
    pub custom_commands: Vec<CustomCommandExport>,
    pub links: Vec<LinkExport>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
}

#[derive(Debug, Serialize)]
pub struct CommandsExport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_check: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_start_check: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infra_check: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuild: Option<CommandDetailExport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuild_infra: Option<CommandDetailExport>,
}

#[derive(Debug, Serialize)]
pub struct CommandDetailExport {
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct CustomCommandExport {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub requires_confirmation: bool,
    pub parameters: Vec<CommandParamExport>,
}

#[derive(Debug, Serialize)]
pub struct CommandParamExport {
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    pub required: bool,
    pub param_type: String,
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct LinkExport {
    pub label: String,
    pub url: String,
    pub link_type: String,
}

#[derive(Debug, Serialize)]
pub struct DependencyExport {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_type: Option<String>,
}

// ── Database Row Types ──────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct AppRow {
    name: String,
    description: Option<String>,
    tags: Option<Value>,
}

#[derive(sqlx::FromRow)]
struct VarRow {
    name: String,
    value: String,
    description: Option<String>,
    is_secret: bool,
}

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: DbUuid,
    name: String,
    description: Option<String>,
    color: Option<String>,
    display_order: i32,
}

#[derive(sqlx::FromRow)]
struct ComponentRow {
    id: DbUuid,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    component_type: String,
    icon: Option<String>,
    group_id: Option<DbUuid>,
    host: Option<String>,
    check_cmd: Option<String>,
    start_cmd: Option<String>,
    stop_cmd: Option<String>,
    integrity_check_cmd: Option<String>,
    post_start_check_cmd: Option<String>,
    infra_check_cmd: Option<String>,
    rebuild_cmd: Option<String>,
    rebuild_infra_cmd: Option<String>,
    check_interval_seconds: i32,
    start_timeout_seconds: i32,
    stop_timeout_seconds: i32,
    is_optional: bool,
    position_x: Option<f32>,
    position_y: Option<f32>,
}

#[derive(sqlx::FromRow)]
struct CustomCmdRow {
    id: DbUuid,
    component_id: DbUuid,
    name: String,
    command: String,
    description: Option<String>,
    requires_confirmation: bool,
}

#[derive(sqlx::FromRow)]
struct CmdParamRow {
    command_id: DbUuid,
    name: String,
    description: Option<String>,
    default_value: Option<String>,
    validation_regex: Option<String>,
    required: bool,
    param_type: String,
    enum_values: Option<Value>,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    component_id: DbUuid,
    label: String,
    url: String,
    link_type: String,
}

#[derive(sqlx::FromRow)]
struct DepRow {
    from_component_id: DbUuid,
    to_component_id: DbUuid,
}

// ── Export Endpoint ─────────────────────────────────────────────────

/// GET /api/v1/apps/:id/export
/// Export application in v4 native JSON format.
pub async fn export_app_json(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ExportQuery>,
) -> Result<Json<Value>, ApiError> {
    // Check permission (View is sufficient for export)
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let include_secrets = params.include_secrets.unwrap_or(false);

    // Fetch application
    let app = sqlx::query_as::<_, AppRow>(
        "SELECT name, description, tags FROM applications WHERE id = $1",
    )
    .bind(app_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;

    // Parse tags
    let tags: Vec<String> = app
        .tags
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    // Fetch variables
    let var_rows = sqlx::query_as::<_, VarRow>(
        "SELECT name, value, description, is_secret FROM app_variables WHERE application_id = $1 ORDER BY name",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let variables: Vec<VariableExport> = var_rows
        .into_iter()
        .map(|v| VariableExport {
            name: v.name,
            value: if v.is_secret && !include_secrets {
                "***".to_string()
            } else {
                v.value
            },
            description: v.description,
            is_secret: v.is_secret,
        })
        .collect();

    // Fetch groups and build ID → name map
    let group_rows = sqlx::query_as::<_, GroupRow>(
        "SELECT id, name, description, color, display_order FROM component_groups WHERE application_id = $1 ORDER BY display_order",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let group_id_to_name: HashMap<DbUuid, String> =
        group_rows.iter().map(|g| (g.id, g.name.clone())).collect();

    let groups: Vec<GroupExport> = group_rows
        .into_iter()
        .map(|g| GroupExport {
            name: g.name,
            description: g.description,
            color: g.color,
            display_order: g.display_order,
        })
        .collect();

    // Fetch components
    let comp_rows = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, name, display_name, description, component_type, icon, group_id, host,
               check_cmd, start_cmd, stop_cmd, integrity_check_cmd, post_start_check_cmd,
               infra_check_cmd, rebuild_cmd, rebuild_infra_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds,
               is_optional, position_x, position_y
        FROM components WHERE application_id = $1 ORDER BY name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Build component ID → name map for dependencies
    let comp_id_to_name: HashMap<DbUuid, String> =
        comp_rows.iter().map(|c| (c.id, c.name.clone())).collect();

    // Fetch all custom commands
    let cmd_rows = sqlx::query_as::<_, CustomCmdRow>(
        r#"
        SELECT cc.id, cc.component_id, cc.name, cc.command, cc.description, cc.requires_confirmation
        FROM component_commands cc
        JOIN components c ON c.id = cc.component_id
        WHERE c.application_id = $1
        ORDER BY cc.name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Fetch all command parameters
    let param_rows = sqlx::query_as::<_, CmdParamRow>(
        r#"
        SELECT cip.command_id, cip.name, cip.description, cip.default_value,
               cip.validation_regex, cip.required, cip.param_type, cip.enum_values
        FROM command_input_params cip
        JOIN component_commands cc ON cc.id = cip.command_id
        JOIN components c ON c.id = cc.component_id
        WHERE c.application_id = $1
        ORDER BY cip.display_order
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Group parameters by command ID
    let mut params_by_cmd: HashMap<DbUuid, Vec<CommandParamExport>> = HashMap::new();
    for p in param_rows {
        let param = CommandParamExport {
            name: p.name,
            description: p.description,
            default_value: p.default_value,
            validation_regex: p.validation_regex,
            required: p.required,
            param_type: p.param_type,
            enum_values: p.enum_values.and_then(|v| serde_json::from_value(v).ok()),
        };
        params_by_cmd.entry(p.command_id).or_default().push(param);
    }

    // Group custom commands by component ID
    let mut cmds_by_comp: HashMap<DbUuid, Vec<CustomCommandExport>> = HashMap::new();
    for cmd in cmd_rows {
        let custom_cmd = CustomCommandExport {
            name: cmd.name,
            command: cmd.command,
            description: cmd.description,
            requires_confirmation: cmd.requires_confirmation,
            parameters: params_by_cmd.remove(&cmd.id).unwrap_or_default(),
        };
        cmds_by_comp
            .entry(cmd.component_id)
            .or_default()
            .push(custom_cmd);
    }

    // Fetch all links
    let link_rows = sqlx::query_as::<_, LinkRow>(
        r#"
        SELECT cl.component_id, cl.label, cl.url, cl.link_type
        FROM component_links cl
        JOIN components c ON c.id = cl.component_id
        WHERE c.application_id = $1
        ORDER BY cl.display_order
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Group links by component ID
    let mut links_by_comp: HashMap<DbUuid, Vec<LinkExport>> = HashMap::new();
    for link in link_rows {
        let link_export = LinkExport {
            label: link.label,
            url: link.url,
            link_type: link.link_type,
        };
        links_by_comp
            .entry(link.component_id)
            .or_default()
            .push(link_export);
    }

    // Build component exports
    let components: Vec<ComponentExport> = comp_rows
        .into_iter()
        .map(|c| {
            let commands =
                CommandsExport {
                    check: c.check_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: Some(c.check_interval_seconds),
                    }),
                    start: c.start_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: Some(c.start_timeout_seconds),
                    }),
                    stop: c.stop_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: Some(c.stop_timeout_seconds),
                    }),
                    integrity_check: c.integrity_check_cmd.as_ref().map(|cmd| {
                        CommandDetailExport {
                            cmd: cmd.clone(),
                            timeout_seconds: None,
                        }
                    }),
                    post_start_check: c.post_start_check_cmd.as_ref().map(|cmd| {
                        CommandDetailExport {
                            cmd: cmd.clone(),
                            timeout_seconds: None,
                        }
                    }),
                    infra_check: c.infra_check_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: None,
                    }),
                    rebuild: c.rebuild_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: None,
                    }),
                    rebuild_infra: c.rebuild_infra_cmd.as_ref().map(|cmd| CommandDetailExport {
                        cmd: cmd.clone(),
                        timeout_seconds: None,
                    }),
                };

            ComponentExport {
                name: c.name.clone(),
                display_name: c.display_name,
                description: c.description,
                component_type: c.component_type,
                icon: c.icon,
                group: c
                    .group_id
                    .and_then(|gid| group_id_to_name.get(&gid).cloned()),
                host: c.host,
                commands,
                custom_commands: cmds_by_comp.remove(&c.id).unwrap_or_default(),
                links: links_by_comp.remove(&c.id).unwrap_or_default(),
                position_x: c.position_x,
                position_y: c.position_y,
                check_interval_seconds: c.check_interval_seconds,
                start_timeout_seconds: c.start_timeout_seconds,
                stop_timeout_seconds: c.stop_timeout_seconds,
                is_optional: c.is_optional,
            }
        })
        .collect();

    // Fetch dependencies
    let dep_rows = sqlx::query_as::<_, DepRow>(
        "SELECT from_component_id, to_component_id FROM dependencies WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let dependencies: Vec<DependencyExport> = dep_rows
        .into_iter()
        .filter_map(|d| {
            let from = comp_id_to_name.get(&d.from_component_id)?.clone();
            let to = comp_id_to_name.get(&d.to_component_id)?.clone();
            Some(DependencyExport {
                from,
                to,
                dep_type: None, // dep_type column not in DB yet
            })
        })
        .collect();

    let export = ExportedApplication {
        format_version: "4.0",
        exported_at: chrono::Utc::now(),
        application: ApplicationExport {
            name: app.name,
            description: app.description,
            tags,
            variables,
            groups,
            components,
            dependencies,
        },
    };

    Ok(Json(json!(export)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands_export_serialization() {
        let commands = CommandsExport {
            check: Some(CommandDetailExport {
                cmd: "pgrep nginx".to_string(),
                timeout_seconds: Some(30),
            }),
            start: Some(CommandDetailExport {
                cmd: "systemctl start nginx".to_string(),
                timeout_seconds: Some(120),
            }),
            stop: None,
            integrity_check: None,
            post_start_check: None,
            infra_check: None,
            rebuild: None,
            rebuild_infra: None,
        };

        let json = serde_json::to_value(&commands).unwrap();
        assert!(json.get("check").is_some());
        assert!(json.get("start").is_some());
        // None values should be skipped
        assert!(json.get("stop").is_none());
        assert!(json.get("integrity_check").is_none());
    }

    #[test]
    fn test_dependency_export_serialization() {
        let dep = DependencyExport {
            from: "webserver".to_string(),
            to: "database".to_string(),
            dep_type: None,
        };

        let json = serde_json::to_value(&dep).unwrap();
        assert_eq!(json["from"], "webserver");
        assert_eq!(json["to"], "database");
        // dep_type None should be skipped
        assert!(json.get("dep_type").is_none());
    }
}
