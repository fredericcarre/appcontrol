/// E2E Test: SAML 2.0 Authentication Flow
///
/// Validates:
/// - SP metadata endpoint returns valid XML with entity ID and ACS URL
/// - SAML login redirects to IdP SSO URL with AuthnRequest
/// - ACS endpoint processes SAML response and returns JWT via HTML redirect
/// - SAML group → team mapping CRUD (admin API)
/// - Group sync: user added to teams based on SAML group claims
/// - Group removal: user removed from teams when group claim disappears
/// - Admin group mapping grants admin role
use super::*;

#[cfg(test)]
mod test_saml_auth {
    use super::*;
    use base64::Engine;

    /// Helper: Create a TestContext with SAML enabled.
    async fn saml_context() -> TestContext {
        let ctx = TestContext::new_with_saml(
            "https://idp.example.com/sso",
            "appcontrol-test-sp",
            // ACS URL will be filled with actual test server URL
        )
        .await;
        ctx
    }

    #[tokio::test]
    async fn test_saml_metadata_endpoint() {
        let ctx = saml_context().await;

        let resp = ctx.get_anonymous("/api/v1/auth/saml/metadata").await;
        assert_eq!(resp.status().as_u16(), 200, "Metadata should return 200");

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(content_type.contains("xml"), "Metadata should be XML");

        let body = resp.text().await.unwrap();
        assert!(
            body.contains("EntityDescriptor"),
            "Should contain EntityDescriptor"
        );
        assert!(
            body.contains("appcontrol-test-sp"),
            "Should contain SP entity ID"
        );
        assert!(
            body.contains("AssertionConsumerService"),
            "Should contain ACS definition"
        );
        assert!(
            body.contains("urn:oasis:names:tc:SAML:2.0"),
            "Should reference SAML 2.0"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_login_redirects_to_idp() {
        let ctx = saml_context().await;

        let resp = ctx
            .client_no_redirect()
            .get(format!("{}/api/v1/auth/saml/login", ctx.api_url))
            .send()
            .await
            .unwrap();

        // Should be a 307 redirect
        assert!(
            resp.status().is_redirection(),
            "Login should redirect, got {}",
            resp.status()
        );

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            location.starts_with("https://idp.example.com/sso?"),
            "Should redirect to IdP SSO URL, got: {}",
            location
        );
        assert!(
            location.contains("SAMLRequest="),
            "Redirect should contain SAMLRequest parameter"
        );
        assert!(
            location.contains("RelayState="),
            "Redirect should contain RelayState parameter"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_acs_processes_response() {
        let ctx = saml_context().await;

        // Build a mock SAML Response (base64 encoded XML)
        let saml_response_xml = format!(
            r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                             xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                             Version="2.0" ID="_response_1">
                <saml:Assertion Version="2.0" ID="_assertion_1">
                    <saml:Subject>
                        <saml:NameID>jean.dupont@example.com</saml:NameID>
                    </saml:Subject>
                    <saml:AttributeStatement>
                        <saml:Attribute Name="email">
                            <saml:AttributeValue>jean.dupont@example.com</saml:AttributeValue>
                        </saml:Attribute>
                        <saml:Attribute Name="displayName">
                            <saml:AttributeValue>Jean Dupont</saml:AttributeValue>
                        </saml:Attribute>
                        <saml:Attribute Name="memberOf">
                            <saml:AttributeValue>CN=APP_PAYMENTS_OPS,OU=Groups,DC=corp</saml:AttributeValue>
                        </saml:Attribute>
                    </saml:AttributeStatement>
                </saml:Assertion>
            </samlp:Response>"#
        );

        let encoded =
            base64::engine::general_purpose::STANDARD.encode(saml_response_xml.as_bytes());

        // POST to ACS endpoint (form-encoded, as per SAML HTTP-POST binding)
        let resp = ctx
            .post_form_anonymous(
                "/api/v1/auth/saml/acs",
                &[
                    ("SAMLResponse", encoded.as_str()),
                    ("RelayState", "/dashboard"),
                ],
            )
            .await;

        assert_eq!(resp.status().as_u16(), 200, "ACS should return 200 HTML");

        let body = resp.text().await.unwrap();
        assert!(
            body.contains("localStorage.setItem('token'"),
            "Should set JWT token in localStorage"
        );
        assert!(body.contains("/dashboard"), "Should redirect to RelayState");

        // Verify user was created in the database
        let user = sqlx::query_as::<_, (appcontrol_backend::db::DbUuid, String, String)>(
            "SELECT id, email, role FROM users WHERE email = 'jean.dupont@example.com'",
        )
        .fetch_optional(&ctx.db_pool)
        .await
        .unwrap();

        assert!(user.is_some(), "SAML user should be created in database");
        let (user_id, email, role) = user.unwrap();
        assert_eq!(email, "jean.dupont@example.com");
        assert_eq!(role, "viewer", "Default role should be viewer");

        // Verify saml_name_id was set
        let name_id =
            sqlx::query_scalar::<_, Option<String>>("SELECT saml_name_id FROM users WHERE id = $1")
                .bind(bind_id(user_id))
                .fetch_one(&ctx.db_pool)
                .await
                .unwrap();
        assert_eq!(name_id, Some("jean.dupont@example.com".to_string()));

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_group_mapping_crud() {
        let ctx = saml_context().await;

        // Create a team first
        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Payments-Ops",
                    "description": "Payment operations team"
                }),
            )
            .await;
        assert!(resp.status().is_success(), "Team creation should succeed");
        let team: Value = resp.json().await.unwrap();
        let team_id = team["id"].as_str().unwrap();

        // Create a SAML group mapping
        let resp = ctx
            .post(
                "/api/v1/saml/group-mappings",
                json!({
                    "saml_group": "CN=APP_PAYMENTS_OPS,OU=Groups,DC=corp",
                    "team_id": team_id,
                    "default_role": "operator"
                }),
            )
            .await;
        assert_eq!(
            resp.status().as_u16(),
            200,
            "Group mapping creation should succeed"
        );
        let mapping: Value = resp.json().await.unwrap();
        assert_eq!(
            mapping["saml_group"],
            "CN=APP_PAYMENTS_OPS,OU=Groups,DC=corp"
        );
        assert_eq!(mapping["default_role"], "operator");

        // List group mappings
        let resp = ctx.get("/api/v1/saml/group-mappings").await;
        assert_eq!(resp.status().as_u16(), 200);
        let mappings: Vec<Value> = resp.json().await.unwrap();
        assert!(!mappings.is_empty(), "Should have at least one mapping");
        let found = mappings
            .iter()
            .any(|m| m["saml_group"] == "CN=APP_PAYMENTS_OPS,OU=Groups,DC=corp");
        assert!(found, "Should find the created mapping");

        // Delete group mapping
        let mapping_id = mapping["id"].as_str().unwrap();
        let resp = ctx
            .delete_as(
                "admin",
                &format!("/api/v1/saml/group-mappings/{mapping_id}"),
            )
            .await;
        assert_eq!(resp.status().as_u16(), 204, "Delete should return 204");

        // Verify deletion
        let resp = ctx.get("/api/v1/saml/group-mappings").await;
        let mappings: Vec<Value> = resp.json().await.unwrap();
        let found = mappings
            .iter()
            .any(|m| m["id"].as_str() == Some(mapping_id));
        assert!(!found, "Mapping should be deleted");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_group_sync_adds_to_team() {
        let ctx = saml_context().await;

        // Create team and mapping
        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Payments-Operators",
                    "description": "SAML synced team"
                }),
            )
            .await;
        let team: Value = resp.json().await.unwrap();
        let team_id = team["id"].as_str().unwrap();

        ctx.post(
            "/api/v1/saml/group-mappings",
            json!({
                "saml_group": "CN=PAYMENTS_OPS,DC=corp",
                "team_id": team_id,
                "default_role": "operator"
            }),
        )
        .await;

        // SAML login with matching group
        let saml_response_xml = r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                                                   xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                                                   Version="2.0" ID="_resp_sync">
            <saml:Assertion Version="2.0" ID="_assert_sync">
                <saml:Subject>
                    <saml:NameID>sync.user@example.com</saml:NameID>
                </saml:Subject>
                <saml:AttributeStatement>
                    <saml:Attribute Name="email">
                        <saml:AttributeValue>sync.user@example.com</saml:AttributeValue>
                    </saml:Attribute>
                    <saml:Attribute Name="displayName">
                        <saml:AttributeValue>Sync User</saml:AttributeValue>
                    </saml:Attribute>
                    <saml:Attribute Name="memberOf">
                        <saml:AttributeValue>CN=PAYMENTS_OPS,DC=corp</saml:AttributeValue>
                    </saml:Attribute>
                </saml:AttributeStatement>
            </saml:Assertion>
        </samlp:Response>"#;

        let encoded =
            base64::engine::general_purpose::STANDARD.encode(saml_response_xml.as_bytes());
        ctx.post_form_anonymous(
            "/api/v1/auth/saml/acs",
            &[("SAMLResponse", &encoded), ("RelayState", "/")],
        )
        .await;

        // Verify user was added to the team
        let user_id = sqlx::query_scalar::<_, appcontrol_backend::db::DbUuid>(
            "SELECT id FROM users WHERE email = 'sync.user@example.com'",
        )
        .fetch_one(&ctx.db_pool)
        .await
        .unwrap();

        let team_id_uuid: Uuid = team_id.parse().unwrap();
        let is_member = {
            let count: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
            )
            .bind(bind_id(team_id_uuid))
            .bind(bind_id(user_id))
            .fetch_one(&ctx.db_pool)
            .await
            .unwrap();
            count > 0
        };

        assert!(
            is_member,
            "User should be added to the team via SAML group sync"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_group_removal_on_login() {
        let ctx = saml_context().await;

        // Create two teams and mappings
        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Team-A", "description": "Team A"
                }),
            )
            .await;
        let team_a: Value = resp.json().await.unwrap();
        let team_a_id = team_a["id"].as_str().unwrap();

        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Team-B", "description": "Team B"
                }),
            )
            .await;
        let team_b: Value = resp.json().await.unwrap();
        let team_b_id = team_b["id"].as_str().unwrap();

        ctx.post(
            "/api/v1/saml/group-mappings",
            json!({
                "saml_group": "GROUP-A",
                "team_id": team_a_id,
            }),
        )
        .await;
        ctx.post(
            "/api/v1/saml/group-mappings",
            json!({
                "saml_group": "GROUP-B",
                "team_id": team_b_id,
            }),
        )
        .await;

        // First login: user has both groups
        let make_saml_resp = |groups: &[&str]| {
            let group_values = groups
                .iter()
                .map(|g| format!(r#"<saml:AttributeValue>{g}</saml:AttributeValue>"#))
                .collect::<Vec<_>>()
                .join("\n                            ");
            let xml = format!(
                r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                                 xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                                 Version="2.0" ID="_resp_removal">
                    <saml:Assertion Version="2.0" ID="_assert_removal">
                        <saml:Subject>
                            <saml:NameID>removal.user@example.com</saml:NameID>
                        </saml:Subject>
                        <saml:AttributeStatement>
                            <saml:Attribute Name="email">
                                <saml:AttributeValue>removal.user@example.com</saml:AttributeValue>
                            </saml:Attribute>
                            <saml:Attribute Name="displayName">
                                <saml:AttributeValue>Removal User</saml:AttributeValue>
                            </saml:Attribute>
                            <saml:Attribute Name="memberOf">
                                {group_values}
                            </saml:Attribute>
                        </saml:AttributeStatement>
                    </saml:Assertion>
                </samlp:Response>"#
            );
            base64::engine::general_purpose::STANDARD.encode(xml.as_bytes())
        };

        // Login 1: both groups
        let encoded = make_saml_resp(&["GROUP-A", "GROUP-B"]);
        ctx.post_form_anonymous(
            "/api/v1/auth/saml/acs",
            &[("SAMLResponse", &encoded), ("RelayState", "/")],
        )
        .await;

        let user_id = sqlx::query_scalar::<_, appcontrol_backend::db::DbUuid>(
            "SELECT id FROM users WHERE email = 'removal.user@example.com'",
        )
        .fetch_one(&ctx.db_pool)
        .await
        .unwrap();

        let team_a_uuid: Uuid = team_a_id.parse().unwrap();
        let team_b_uuid: Uuid = team_b_id.parse().unwrap();

        // Should be member of both teams
        let in_a = {
            let c: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
            )
            .bind(bind_id(team_a_uuid))
            .bind(bind_id(user_id))
            .fetch_one(&ctx.db_pool).await.unwrap();
            c > 0
        };
        let in_b = {
            let c: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
            )
            .bind(bind_id(team_b_uuid))
            .bind(bind_id(user_id))
            .fetch_one(&ctx.db_pool).await.unwrap();
            c > 0
        };
        assert!(in_a, "Should be in Team-A after first login");
        assert!(in_b, "Should be in Team-B after first login");

        // Login 2: only GROUP-A (GROUP-B removed)
        let encoded = make_saml_resp(&["GROUP-A"]);
        ctx.post_form_anonymous(
            "/api/v1/auth/saml/acs",
            &[("SAMLResponse", &encoded), ("RelayState", "/")],
        )
        .await;

        let in_a = {
            let c: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
            )
            .bind(bind_id(team_a_uuid))
            .bind(bind_id(user_id))
            .fetch_one(&ctx.db_pool).await.unwrap();
            c > 0
        };
        let in_b = {
            let c: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
            )
            .bind(bind_id(team_b_uuid))
            .bind(bind_id(user_id))
            .fetch_one(&ctx.db_pool).await.unwrap();
            c > 0
        };
        assert!(in_a, "Should still be in Team-A after second login");
        assert!(!in_b, "Should be removed from Team-B after second login");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_saml_admin_group_grants_admin_role() {
        let ctx = TestContext::new_with_saml_admin(
            "https://idp.example.com/sso",
            "appcontrol-test-sp",
            "CN=APPCONTROL_ADMINS,DC=corp",
        )
        .await;

        let saml_response_xml = r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                                                   xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion"
                                                   Version="2.0" ID="_resp_admin">
            <saml:Assertion Version="2.0" ID="_assert_admin">
                <saml:Subject>
                    <saml:NameID>admin.user@example.com</saml:NameID>
                </saml:Subject>
                <saml:AttributeStatement>
                    <saml:Attribute Name="email">
                        <saml:AttributeValue>admin.user@example.com</saml:AttributeValue>
                    </saml:Attribute>
                    <saml:Attribute Name="displayName">
                        <saml:AttributeValue>Admin User</saml:AttributeValue>
                    </saml:Attribute>
                    <saml:Attribute Name="memberOf">
                        <saml:AttributeValue>CN=APPCONTROL_ADMINS,DC=corp</saml:AttributeValue>
                    </saml:Attribute>
                </saml:AttributeStatement>
            </saml:Assertion>
        </samlp:Response>"#;

        let encoded =
            base64::engine::general_purpose::STANDARD.encode(saml_response_xml.as_bytes());
        ctx.post_form_anonymous(
            "/api/v1/auth/saml/acs",
            &[("SAMLResponse", &encoded), ("RelayState", "/")],
        )
        .await;

        // Verify user was created with admin role
        let role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM users WHERE email = 'admin.user@example.com'",
        )
        .fetch_one(&ctx.db_pool)
        .await
        .unwrap();

        assert_eq!(role, "admin", "User in admin group should get admin role");

        ctx.cleanup().await;
    }
}
