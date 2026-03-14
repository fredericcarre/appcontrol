# Plan: Dynamic Config Push & Application Suspension

## Objectif

1. **Push dynamique** : L'agent reçoit automatiquement les configs quand une map est importée/modifiée
2. **Suspension de map** : Possibilité de "mettre en pause" une application (l'agent arrête les checks)

## Changements requis

### 1. Migration SQL

```sql
-- V031__application_suspension.sql
ALTER TABLE applications ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE applications ADD COLUMN suspended_at TIMESTAMPTZ;
ALTER TABLE applications ADD COLUMN suspended_by UUID REFERENCES users(id);

-- Index pour filtrer les apps actives
CREATE INDEX idx_applications_suspended ON applications (organization_id, is_suspended);
```

### 2. Modifier `send_config_to_agent`

Exclure les composants des applications suspendues :

```sql
SELECT c.id, c.name, c.check_cmd, ...
FROM components c
JOIN applications a ON c.application_id = a.id
WHERE c.agent_id = $1
  AND a.is_suspended = false  -- NOUVEAU: exclure les apps suspendues
```

### 3. Fonction utilitaire `push_config_to_affected_agents`

Créer une fonction réutilisable qui :
1. Trouve tous les agents affectés par une liste de component_ids ou application_id
2. Envoie un UpdateConfig à chaque agent

```rust
// Dans backend/src/websocket/mod.rs ou un nouveau module
pub async fn push_config_to_affected_agents(
    state: &Arc<AppState>,
    application_id: Option<Uuid>,
    component_ids: Option<Vec<Uuid>>,
) {
    // 1. Trouver les agent_ids distincts
    let agent_ids: Vec<Uuid> = if let Some(app_id) = application_id {
        sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components
             WHERE application_id = $1 AND agent_id IS NOT NULL"
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else if let Some(comp_ids) = component_ids {
        sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components
             WHERE id = ANY($1) AND agent_id IS NOT NULL"
        )
        .bind(&comp_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        return;
    };

    // 2. Envoyer UpdateConfig à chaque agent
    for agent_id in agent_ids {
        if state.ws_hub.is_agent_connected(agent_id) {
            send_config_to_agent(state, agent_id).await;
        }
    }
}
```

### 4. Points d'appel du push dynamique

| Événement | Action |
|-----------|--------|
| Import wizard termine | `push_config_to_affected_agents(app_id)` |
| POST /components | `push_config_to_affected_agents(component_ids)` |
| PUT /components/:id | `push_config_to_affected_agents(component_ids)` |
| DELETE /components/:id | `push_config_to_affected_agents(component_ids)` |
| PUT /components/:id (agent_id change) | Push aux 2 agents (ancien + nouveau) |
| POST /applications/:id/suspend | `push_config_to_affected_agents(app_id)` |
| POST /applications/:id/resume | `push_config_to_affected_agents(app_id)` |

### 5. API Suspend/Resume

```rust
// PUT /api/v1/applications/:id/suspend
pub async fn suspend_application(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<Uuid>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Application>, AppError> {
    // Vérifier permission >= manage

    sqlx::query(
        "UPDATE applications
         SET is_suspended = true, suspended_at = now(), suspended_by = $2, updated_at = now()
         WHERE id = $1"
    )
    .bind(app_id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    // Notifier les agents
    push_config_to_affected_agents(&state, Some(app_id), None).await;

    // Log action
    log_action(&state.db, user.id, "suspend_application", "application", app_id, json!({})).await;

    // Retourner l'app mise à jour
    get_application(&state.db, app_id).await
}

// PUT /api/v1/applications/:id/resume
pub async fn resume_application(...) {
    // Inverse de suspend
}
```

### 6. Comportement côté Agent

Quand l'agent reçoit un `UpdateConfig` :
- Il compare avec sa config actuelle
- Les composants absents de la nouvelle config sont **retirés** du scheduler
- Les nouveaux composants sont **ajoutés** au scheduler
- Les composants existants sont **mis à jour** si les paramètres changent

C'est déjà implémenté dans `scheduler.update_components()` :
```rust
pub async fn update_components(&self, configs: Vec<ComponentConfig>) {
    // ...
    // Remove stale check state for components no longer in the config
    check_state.retain(|id, _| new_ids.contains(id));
    // Replace component configs
    components.clear();
    for config in configs {
        components.insert(config.component_id, config);
    }
}
```

### 7. Frontend

- Ajouter bouton "Suspend" / "Resume" dans la toolbar de la map
- Afficher un badge "SUSPENDED" sur les maps suspendues
- Les composants d'une map suspendue affichent un état visuel distinct

## Fichiers à modifier

| Fichier | Modification |
|---------|--------------|
| `migrations/V031__application_suspension.sql` | Nouveau |
| `crates/backend/src/websocket/mod.rs` | Modifier `send_config_to_agent`, ajouter `push_config_to_affected_agents` |
| `crates/backend/src/api/apps.rs` | Ajouter `suspend_application`, `resume_application` |
| `crates/backend/src/api/import_wizard.rs` | Appeler `push_config_to_affected_agents` après import |
| `crates/backend/src/api/components.rs` | Appeler `push_config_to_affected_agents` dans CRUD |
| `frontend/src/api/apps.ts` | Ajouter hooks `useSuspendApp`, `useResumeApp` |
| `frontend/src/components/maps/MapToolbar.tsx` | Ajouter bouton Suspend/Resume |
| `frontend/src/pages/DashboardPage.tsx` | Afficher badge SUSPENDED |

## Tests

1. Importer une map → agent reçoit immédiatement la config
2. Suspendre une map → agent retire les composants du scheduler
3. Reprendre une map → agent reçoit à nouveau les composants
4. Modifier un composant → agent reçoit la config mise à jour
5. Changer l'agent_id d'un composant → les 2 agents sont notifiés
