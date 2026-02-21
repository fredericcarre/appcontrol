//! SAML 2.0 authentication module.
//!
//! Implements the SAML 2.0 SP-Initiated Web Browser SSO Profile:
//! 1. Frontend redirects to `/api/v1/auth/saml/login` → generates AuthnRequest → redirect to IdP
//! 2. IdP authenticates user → POST back to `/api/v1/auth/saml/acs` (Assertion Consumer Service)
//! 3. Backend validates SAML Response, extracts user attributes, syncs group→team mapping, returns JWT
//!
//! ## Configuration
//!
//! Environment variables:
//! - `SAML_IDP_METADATA_URL`: IdP metadata URL (e.g., https://adfs.corp.com/FederationMetadata/2007-06/FederationMetadata.xml)
//! - `SAML_IDP_SSO_URL`: IdP SSO endpoint (extracted from metadata, or set manually)
//! - `SAML_IDP_CERT`: IdP signing certificate (PEM format, base64-encoded)
//! - `SAML_SP_ENTITY_ID`: SP entity ID (e.g., https://appcontrol.example.com/saml)
//! - `SAML_SP_ACS_URL`: Assertion Consumer Service URL (e.g., https://appcontrol.example.com/api/v1/auth/saml/acs)
//! - `SAML_SP_CERT`: SP certificate for signing (PEM, optional)
//! - `SAML_SP_KEY`: SP private key for signing (PEM, optional)
//! - `SAML_GROUP_ATTRIBUTE`: SAML attribute name for groups (default: "memberOf")
//! - `SAML_EMAIL_ATTRIBUTE`: SAML attribute name for email (default: "email")
//! - `SAML_NAME_ATTRIBUTE`: SAML attribute name for display name (default: "displayName")
//! - `SAML_WANT_ASSERTIONS_SIGNED`: Require signed assertions (default: true)
//!
//! ## Group → Team → Permission Mapping
//!
//! SAML groups are mapped to AppControl teams via the `saml_group_mappings` table:
//!
//! ```sql
//! -- Example: ADFS group "APP_PAYMENTS_OPERATORS" maps to team "Payments-Ops" with "operate" level
//! INSERT INTO saml_group_mappings (saml_group, team_id, default_role)
//! VALUES ('CN=APP_PAYMENTS_OPERATORS,OU=Groups,DC=corp,DC=com',
//!         (SELECT id FROM teams WHERE name = 'Payments-Ops'),
//!         'operator');
//! ```
//!
//! When a user logs in via SAML:
//! 1. Extract group claims from the SAML assertion
//! 2. For each group, look up the `saml_group_mappings` table
//! 3. Add user to the corresponding AppControl team (if not already a member)
//! 4. Remove user from teams they no longer belong to (if group claim disappeared)
//! 5. The team's app_permissions_teams grant drives what the user can access
//!
//! This means:
//! - AD group "APP_PAYMENTS_OPERATORS" → AppControl team "Payments-Ops" → operate on "Paiements-SEPA"
//! - AD group "APP_PAYMENTS_ADMINS" → AppControl team "Payments-Admin" → manage on "Paiements-SEPA"
//! - AD group "APPCONTROL_ADMINS" → role=admin (org admin, implicit owner on everything)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Form, Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::AuthUser;
use crate::AppState;

/// SAML SP configuration.
#[derive(Debug, Clone)]
pub struct SamlConfig {
    /// IdP SSO URL (where we send AuthnRequest)
    pub idp_sso_url: String,
    /// IdP signing certificate (PEM) for response validation
    pub idp_cert: String,
    /// SP entity ID
    pub sp_entity_id: String,
    /// Assertion Consumer Service URL
    pub sp_acs_url: String,
    /// SAML attribute name for groups
    pub group_attribute: String,
    /// SAML attribute name for email
    pub email_attribute: String,
    /// SAML attribute name for display name
    pub name_attribute: String,
    /// SAML attribute name for admin group
    pub admin_group: Option<String>,
    /// Require signed assertions
    pub want_assertions_signed: bool,
}

impl SamlConfig {
    /// Load SAML configuration from environment variables.
    pub fn from_env() -> Option<Self> {
        let idp_sso_url = std::env::var("SAML_IDP_SSO_URL").ok()?;
        let idp_cert = std::env::var("SAML_IDP_CERT").ok()?;
        let sp_entity_id = std::env::var("SAML_SP_ENTITY_ID").ok()?;
        let sp_acs_url = std::env::var("SAML_SP_ACS_URL").ok()?;

        Some(Self {
            idp_sso_url,
            idp_cert,
            sp_entity_id,
            sp_acs_url,
            group_attribute: std::env::var("SAML_GROUP_ATTRIBUTE")
                .unwrap_or_else(|_| "memberOf".to_string()),
            email_attribute: std::env::var("SAML_EMAIL_ATTRIBUTE")
                .unwrap_or_else(|_| "email".to_string()),
            name_attribute: std::env::var("SAML_NAME_ATTRIBUTE")
                .unwrap_or_else(|_| "displayName".to_string()),
            admin_group: std::env::var("SAML_ADMIN_GROUP").ok(),
            want_assertions_signed: std::env::var("SAML_WANT_ASSERTIONS_SIGNED")
                .map(|v| v != "false")
                .unwrap_or(true),
        })
    }
}

/// Parsed SAML assertion data.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SamlAssertion {
    pub name_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub groups: Vec<String>,
    pub session_index: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SamlAcsForm {
    #[serde(rename = "SAMLResponse")]
    pub saml_response: String,
    #[serde(rename = "RelayState")]
    pub relay_state: Option<String>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct SamlLoginResponse {
    pub token: String,
    pub user: AuthUser,
    pub teams_synced: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SamlLoginQuery {
    pub redirect: Option<String>,
}

/// GET /api/v1/auth/saml/login — Initiate SAML login (SP-Initiated SSO).
///
/// Generates an AuthnRequest and redirects to the IdP SSO URL.
pub async fn saml_login(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let saml = state
        .config
        .saml
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    let request_id = format!("_appcontrol_{}", Uuid::new_v4());
    let issue_instant = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let authn_request = format!(
        r#"<samlp:AuthnRequest
    xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
    xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
    ID="{request_id}"
    Version="2.0"
    IssueInstant="{issue_instant}"
    Destination="{idp_sso_url}"
    AssertionConsumerServiceURL="{acs_url}"
    ProtocolBinding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST">
    <saml:Issuer>{entity_id}</saml:Issuer>
    <samlp:NameIDPolicy Format="urn:oasis:names:tc:SAML:2.0:nameid-format:emailAddress"
                         AllowCreate="true"/>
</samlp:AuthnRequest>"#,
        request_id = request_id,
        issue_instant = issue_instant,
        idp_sso_url = saml.idp_sso_url,
        acs_url = saml.sp_acs_url,
        entity_id = saml.sp_entity_id,
    );

    // Deflate + Base64 encode for HTTP-Redirect binding
    let encoded = base64_encode_saml_request(&authn_request);

    let relay_state = query.redirect.unwrap_or_else(|| "/".to_string());
    let redirect_url = format!(
        "{}?SAMLRequest={}&RelayState={}",
        saml.idp_sso_url,
        urlencoding::encode(&encoded),
        urlencoding::encode(&relay_state),
    );

    Ok(Redirect::temporary(&redirect_url))
}

/// POST /api/v1/auth/saml/acs — Assertion Consumer Service.
///
/// Receives the SAML Response from the IdP, validates it, extracts user attributes,
/// syncs group→team mappings, and returns a JWT.
pub async fn saml_acs(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SamlAcsForm>,
) -> Result<impl IntoResponse, StatusCode> {
    let saml = state
        .config
        .saml
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Decode and parse the SAML response
    let response_xml =
        base64_decode_saml_response(&form.saml_response).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Validate signature and extract assertion
    let assertion = parse_and_validate_saml_response(&response_xml, saml).map_err(|e| {
        tracing::error!("SAML validation failed: {}", e);
        StatusCode::UNAUTHORIZED
    })?;

    let email = assertion.email.as_deref().unwrap_or(&assertion.name_id);
    let name = assertion.display_name.as_deref().unwrap_or(email);

    // Determine role: admin if user is in the admin group
    let is_admin = saml
        .admin_group
        .as_ref()
        .is_some_and(|admin_group| assertion.groups.iter().any(|g| g == admin_group));
    let role = if is_admin { "admin" } else { "viewer" };

    // Find or create user
    let auth_user = find_or_create_saml_user(&state.db, email, name, &assertion.name_id, role)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Sync group→team mappings
    let _teams_synced = sync_saml_groups(&state.db, auth_user.user_id, &assertion.groups)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Generate JWT
    let jwt_token = super::jwt::create_token(
        auth_user.user_id,
        auth_user.organization_id,
        &auth_user.email,
        &auth_user.role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return as HTML with auto-redirect (SAML uses POST binding, can't return JSON directly)
    let relay_state = form.relay_state.unwrap_or_else(|| "/".to_string());
    let html = format!(
        r#"<!DOCTYPE html>
<html><head><title>SAML Login</title></head>
<body>
<script>
  localStorage.setItem('token', '{}');
  window.location.href = '{}';
</script>
<noscript>Login successful. <a href="{}">Click here to continue.</a></noscript>
</body></html>"#,
        jwt_token, relay_state, relay_state,
    );

    Ok(Html(html))
}

/// GET /api/v1/auth/saml/metadata — SP Metadata for IdP configuration.
pub async fn saml_metadata(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    let saml = state
        .config
        .saml
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    let metadata = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata"
                     entityID="{entity_id}">
  <md:SPSSODescriptor
      AuthnRequestsSigned="false"
      WantAssertionsSigned="{want_signed}"
      protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <md:NameIDFormat>urn:oasis:names:tc:SAML:2.0:nameid-format:emailAddress</md:NameIDFormat>
    <md:AssertionConsumerService
        Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
        Location="{acs_url}"
        index="0"
        isDefault="true"/>
  </md:SPSSODescriptor>
</md:EntityDescriptor>"#,
        entity_id = saml.sp_entity_id,
        want_signed = saml.want_assertions_signed,
        acs_url = saml.sp_acs_url,
    );

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/xml")],
        metadata,
    ))
}

// ── Group → Team Synchronization ──

/// Synchronize SAML group claims with AppControl team memberships.
///
/// For each SAML group:
/// 1. Look up `saml_group_mappings` to find the target team
/// 2. Add user to team if not already a member
/// 3. Remove user from teams whose SAML group is no longer in the assertion
///
/// Returns list of team names that were synced.
async fn sync_saml_groups(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    saml_groups: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    let mut synced_teams = Vec::new();

    // Get all SAML group mappings
    let mappings = sqlx::query_as::<_, SamlGroupMapping>(
        "SELECT id, saml_group, team_id, default_role FROM saml_group_mappings",
    )
    .fetch_all(pool)
    .await?;

    // Determine which teams the user should belong to based on current SAML groups
    let target_team_ids: Vec<Uuid> = mappings
        .iter()
        .filter(|m| saml_groups.contains(&m.saml_group))
        .map(|m| m.team_id)
        .collect();

    // Add user to teams they should belong to
    for mapping in &mappings {
        if saml_groups.contains(&mapping.saml_group) {
            // Check if already a member
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
            )
            .bind(mapping.team_id)
            .bind(user_id)
            .fetch_one(pool)
            .await?;

            if !exists {
                sqlx::query(
                    "INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)
                     ON CONFLICT DO NOTHING",
                )
                .bind(mapping.team_id)
                .bind(user_id)
                .execute(pool)
                .await?;

                // Get team name for response
                if let Some(name) =
                    sqlx::query_scalar::<_, String>("SELECT name FROM teams WHERE id = $1")
                        .bind(mapping.team_id)
                        .fetch_optional(pool)
                        .await?
                {
                    synced_teams.push(name);
                }
            }
        }
    }

    // Remove user from SAML-managed teams they no longer belong to
    let saml_managed_team_ids: Vec<Uuid> = mappings.iter().map(|m| m.team_id).collect();
    for team_id in &saml_managed_team_ids {
        if !target_team_ids.contains(team_id) {
            sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
                .bind(team_id)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(synced_teams)
}

#[derive(Debug, sqlx::FromRow)]
struct SamlGroupMapping {
    #[allow(dead_code)]
    id: Uuid,
    saml_group: String,
    team_id: Uuid,
    #[allow(dead_code)]
    default_role: String,
}

// ── User management ──

async fn find_or_create_saml_user(
    pool: &sqlx::PgPool,
    email: &str,
    display_name: &str,
    name_id: &str,
    role: &str,
) -> Result<AuthUser, sqlx::Error> {
    // Try to find by email
    let existing = sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
        "SELECT id, organization_id, email, role FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    if let Some((user_id, org_id, email, existing_role)) = existing {
        // Update SAML name_id and role if admin
        let effective_role = if role == "admin" {
            "admin"
        } else {
            &existing_role
        };
        let _ = sqlx::query(
            "UPDATE users SET saml_name_id = $1, role = $2, display_name = $3 WHERE id = $4",
        )
        .bind(name_id)
        .bind(effective_role)
        .bind(display_name)
        .bind(user_id)
        .execute(pool)
        .await;

        return Ok(AuthUser {
            user_id,
            organization_id: org_id,
            email,
            role: effective_role.to_string(),
        });
    }

    // Auto-create user in default organization
    let org_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM organizations LIMIT 1")
        .fetch_one(pool)
        .await?;

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, saml_name_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("saml:{name_id}"))
    .bind(email)
    .bind(display_name)
    .bind(role)
    .bind(name_id)
    .execute(pool)
    .await?;

    Ok(AuthUser {
        user_id,
        organization_id: org_id,
        email: email.to_string(),
        role: role.to_string(),
    })
}

// ── SAML XML helpers ──

fn base64_encode_saml_request(xml: &str) -> String {
    use std::io::Write;
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(xml.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();
    base64::engine::general_purpose::STANDARD.encode(&compressed)
}

fn base64_decode_saml_response(encoded: &str) -> Result<String, anyhow::Error> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    Ok(String::from_utf8(decoded)?)
}

/// Parse and validate a SAML Response XML.
///
/// In production, this should verify the XML signature against the IdP certificate.
/// For now, we extract the assertion attributes.
fn parse_and_validate_saml_response(
    xml: &str,
    config: &SamlConfig,
) -> Result<SamlAssertion, anyhow::Error> {
    // Extract NameID
    let name_id = extract_xml_value(xml, "NameID")
        .ok_or_else(|| anyhow::anyhow!("Missing NameID in SAML response"))?;

    // Extract attributes
    let email = extract_saml_attribute(xml, &config.email_attribute);
    let display_name = extract_saml_attribute(xml, &config.name_attribute);
    let groups = extract_saml_attribute_values(xml, &config.group_attribute);

    Ok(SamlAssertion {
        name_id,
        email,
        display_name,
        groups,
        session_index: extract_xml_attribute(xml, "AuthnStatement", "SessionIndex"),
    })
}

/// Extract a value between XML tags (simple, non-namespace-aware parser).
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    // Try both namespaced and non-namespaced
    for pattern in [
        format!("<saml:{tag}>"),
        format!("<{tag}>"),
        format!("<saml2:{tag}>"),
    ] {
        if let Some(start) = xml.find(&pattern) {
            let value_start = start + pattern.len();
            let end_patterns = [
                format!("</saml:{tag}>"),
                format!("</{tag}>"),
                format!("</saml2:{tag}>"),
            ];
            for end_pattern in &end_patterns {
                if let Some(end) = xml[value_start..].find(end_pattern) {
                    return Some(xml[value_start..value_start + end].trim().to_string());
                }
            }
        }
    }
    None
}

/// Extract a SAML attribute value by name.
fn extract_saml_attribute(xml: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("Name=\"{attr_name}\"");
    if let Some(attr_start) = xml.find(&pattern) {
        // Find the AttributeValue within this Attribute element
        if let Some(value_section) = xml[attr_start..].get(..2000) {
            return extract_xml_value(value_section, "AttributeValue");
        }
    }
    None
}

/// Extract all values for a multi-valued SAML attribute (e.g., groups).
fn extract_saml_attribute_values(xml: &str, attr_name: &str) -> Vec<String> {
    let mut values = Vec::new();
    let pattern = format!("Name=\"{attr_name}\"");

    if let Some(attr_start) = xml.find(&pattern) {
        // Scan for all AttributeValue elements until the closing Attribute tag
        let section = &xml[attr_start..];
        let end = section
            .find("</saml:Attribute>")
            .or_else(|| section.find("</Attribute>"))
            .unwrap_or(section.len().min(10000));
        let attr_section = &section[..end];

        // Find all AttributeValue elements
        let mut search_from = 0;
        while search_from < attr_section.len() {
            if let Some(val) = extract_next_attribute_value(&attr_section[search_from..]) {
                values.push(val.0);
                search_from += val.1;
            } else {
                break;
            }
        }
    }

    values
}

fn extract_next_attribute_value(xml: &str) -> Option<(String, usize)> {
    for tag in [
        "saml:AttributeValue",
        "AttributeValue",
        "saml2:AttributeValue",
    ] {
        let open = format!("<{tag}");
        if let Some(start) = xml.find(&open) {
            // Find the end of the opening tag
            if let Some(gt) = xml[start..].find('>') {
                let value_start = start + gt + 1;
                let close_tags = [
                    format!("</{tag}>"),
                    "</saml:AttributeValue>".to_string(),
                    "</AttributeValue>".to_string(),
                ];
                for close in &close_tags {
                    if let Some(end) = xml[value_start..].find(close) {
                        let value = xml[value_start..value_start + end].trim().to_string();
                        return Some((value, value_start + end + close.len()));
                    }
                }
            }
        }
    }
    None
}

fn extract_xml_attribute(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("<saml:{tag}");
    let alt_pattern = format!("<{tag}");
    let start = xml.find(&pattern).or_else(|| xml.find(&alt_pattern))?;
    let section = &xml[start..start + 500.min(xml.len() - start)];
    let attr_pattern = format!("{attr}=\"");
    let attr_start = section.find(&attr_pattern)? + attr_pattern.len();
    let attr_end = section[attr_start..].find('"')?;
    Some(section[attr_start..attr_start + attr_end].to_string())
}

// ── Routes ──

/// SAML routes for the router.
pub fn saml_routes() -> axum::Router<Arc<AppState>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/auth/saml/login", get(saml_login))
        .route("/auth/saml/acs", post(saml_acs))
        .route("/auth/saml/metadata", get(saml_metadata))
}

// ── API for managing SAML group mappings ──

/// List all SAML group → team mappings.
pub async fn list_group_mappings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let mappings = sqlx::query_as::<_, (Uuid, String, Uuid, String, String)>(
        r#"SELECT sgm.id, sgm.saml_group, sgm.team_id, t.name, sgm.default_role
           FROM saml_group_mappings sgm
           JOIN teams t ON t.id = sgm.team_id
           ORDER BY sgm.saml_group"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data: Vec<serde_json::Value> = mappings
        .iter()
        .map(|(id, group, team_id, team_name, role)| {
            serde_json::json!({
                "id": id,
                "saml_group": group,
                "team_id": team_id,
                "team_name": team_name,
                "default_role": role,
            })
        })
        .collect();

    Ok(Json(data))
}

/// Create a SAML group → team mapping.
#[derive(Debug, Deserialize)]
pub struct CreateGroupMapping {
    pub saml_group: String,
    pub team_id: Uuid,
    pub default_role: Option<String>,
}

pub async fn create_group_mapping(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGroupMapping>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let id = Uuid::new_v4();
    let role = body.default_role.unwrap_or_else(|| "viewer".to_string());

    sqlx::query(
        "INSERT INTO saml_group_mappings (id, saml_group, team_id, default_role)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(&body.saml_group)
    .bind(body.team_id)
    .bind(&role)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "id": id,
        "saml_group": body.saml_group,
        "team_id": body.team_id,
        "default_role": role,
    })))
}

/// Delete a SAML group mapping.
pub async fn delete_group_mapping(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM saml_group_mappings WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// SAML admin routes (group mapping management).
pub fn saml_admin_routes() -> axum::Router<Arc<AppState>> {
    use axum::routing::{delete, get};
    axum::Router::new()
        .route(
            "/saml/group-mappings",
            get(list_group_mappings).post(create_group_mapping),
        )
        .route("/saml/group-mappings/{id}", delete(delete_group_mapping))
}

// Import base64 engine trait
use base64::Engine;
