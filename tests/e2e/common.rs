 {
        self.post_as("admin", &format!("/api/v1/apps/{}/permissions/users", app_id),
            json!({"user_id": user_id, "permission_level": level})).await;
    }

    pub async fn grant_permission_with_expiry(&self, app_id: Uuid, user_id: Uuid, level: &str, expires: chrono::DateTime<chrono::Utc>) {
        self.post_as("admin", &format!("/api/v1/apps/{}/permissions/users", app_id),
            json!({"user_id": user_id, "permission_level": level, "expires_at": expires.to_rfc3339()})).await;
    }

    pub async fn grant_team_permission(&self, app_id: Uuid, team_id: Uuid, level: &str) {
        self.post_as("admin", &format!("/api/v1/apps/{}/permissions/teams", app_id),
            json!({"team_id": team_id, "permission_level": level})).await;
    }

    pub async fn create_team(&self, name: &str, members: Vec<Uuid>) -> Uuid { todo!() }
    pub async fn create_command(&self, component_id: Uuid, name: &str, cmd: &str, confirm: bool) { todo!() }
    pub async fn create_api_key(&self, name: &str, actions: Vec<&str>) -> String { todo!() }
    pub async fn disconnect_agent(&self, hostname: &str) { todo!() }
    pub async fn reconnect_agent(&self, hostname: &str) { todo!() }

    // ---- Cleanup ----

    pub async fn cleanup(&self) {
        let admin_pool = sqlx::PgPool::connect("postgres://appcontrol:test@localhost:5432/postgres").await.unwrap();
        sqlx::query(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.db_name))
            .execute(&admin_pool).await.unwrap();
    }
}

// ---- Response types ----
#[derive(Debug, serde::Deserialize)] pub struct AppStatus { pub components: Vec<ComponentStatus> }
#[derive(Debug, serde::Deserialize)] pub struct ComponentStatus { pub name: String, pub state: String }
#[derive(Debug, serde::Deserialize)] pub struct App { pub active_site_id: Uuid }
#[derive(Debug, serde::Deserialize)] pub struct StateTransition { pub component_name: String, pub previous_state: String, pub new_state: String, pub trigger_type: String, pub created_at: chrono::DateTime<chrono::Utc> }
#[derive(Debug, serde::Deserialize)] pub struct ActionLog { pub action_type: String, pub target_name: Option<String>, pub user_id: Option<Uuid>, pub api_key_id: Option<Uuid>, pub detail: Value, pub created_at: chrono::DateTime<chrono::Utc> }
#[derive(Debug, serde::Deserialize)] pub struct ConfigVersion { pub change_type: String, pub previous_value: Option<Value>, pub new_value: Option<Value> }
#[derive(Debug, serde::Deserialize)] pub struct SwitchoverLog { pub status: String, pub rto_measured_seconds: Option<f64>, pub phase_prepare_start: Option<String>, pub phase_commit_at: Option<String> }
#[derive(Debug, serde::Deserialize)] pub struct JobStatus { pub state: String, pub failed_component: String }
