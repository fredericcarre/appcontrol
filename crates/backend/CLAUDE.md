# CLAUDE.md - crates/backend

## Purpose
Central API server. Handles REST API, WebSocket push, FSM state machine, DAG sequencing, permissions, DR switchover, diagnostic/rebuild, DORA reports, MCP server, and scheduler integration.

## Dependencies (Cargo.toml)
```toml
[package]
name = "appcontrol-backend"
version = "0.1.0"
edition = "2021"

[dependencies]
appcontrol-common = { path = "../common" }
axum = { version = "0.7", features = ["ws"] }
axum-extra = { version = "0.9", features = ["typed-header"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio", "tls-rustls", "uuid", "chrono", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tower-http = { version = "0.5", features = ["cors", "trace"] }
jsonwebtoken = "9"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
redis = { version = "0.25", features = ["tokio-comp"] }
dashmap = "5"
```

## Architecture
```
backend/src/
├── main.rs                   # Axum server, router, state
├── config.rs                 # Environment config
├── db.rs                     # PostgreSQL pool + Redis client
├── auth/
│   ├── mod.rs                # JWT validation
│   ├── oidc.rs               # OIDC callback
│   ├── saml.rs               # SAML callback
│   └── api_key.rs            # API key auth (for schedulers)
├── middleware/
│   ├── auth.rs               # Extract user from JWT/API key
│   ├── permission.rs         # Check permission on resource
│   └── audit.rs              # Log action BEFORE handler executes
├── api/
│   ├── apps.rs               # CRUD /apps + /apps/:id/start|stop|start-branch
│   ├── components.rs         # CRUD + /components/:id/start|stop|command/:cmd
│   ├── permissions.rs        # /apps/:id/permissions/users|teams|share-links
│   ├── teams.rs              # CRUD /teams + /teams/:id/members
│   ├── switchover.rs         # DR switchover API
│   ├── diagnostic.rs         # POST /apps/:id/diagnose, POST /apps/:id/rebuild
│   ├── reports.rs            # 7 DORA report endpoints
│   ├── orchestration.rs      # Scheduler integration /apps/:id/start|stop|wait-running
│   ├── agents.rs             # Agent management API
│   └── health.rs             # GET /health, /ready
├── core/
│   ├── fsm.rs                # FSM engine (uses common::fsm, writes state_transitions)
│   ├── dag.rs                # DAG builder, cycle detection, topological sort
│   ├── sequencer.rs          # Start/stop sequencing (parallel per level)
│   ├── branch.rs             # Error branch detection
│   ├── switchover.rs         # 6-phase DR engine
│   ├── diagnostic.rs         # 3-level diagnosis + recommendation matrix
│   ├── rebuild.rs            # Rebuild orchestration (DAG order, protection, bastion)
│   └── permissions.rs        # Effective permission resolution
├── websocket/
│   ├── mod.rs                # WebSocket server
│   └── hub.rs                # Subscription management, permission-filtered events
└── mcp/
    └── mod.rs                # MCP server (7 tools)
```

## Critical Implementation Details

### Every API handler MUST:
1. Extract auth (JWT or API key) via middleware
2. INSERT into `action_log` BEFORE executing the action
3. Check effective permission via `core::permissions::effective_permission(user_id, app_id)`
4. Return appropriate HTTP status (403 if no permission, 404 if not found, 409 if conflict)

### FSM Engine (core/fsm.rs)
- Uses `common::fsm::is_valid_transition()` to validate
- On valid transition: INSERT into `state_transitions`, update Redis cache, push WebSocket event
- On invalid transition: return error, do NOT update state
- State stored in Redis (DashMap in-process as fallback) for fast reads; PostgreSQL is the source of truth

### DAG Sequencer (core/sequencer.rs)
- Build DAG from `dependencies` table
- Kahn's algorithm for topological sort → produces levels
- Start: execute each level in parallel (tokio::join!), wait all RUNNING before next level
- Stop: reverse order
- On component failure: SUSPEND (not cancel), return control to operator
- Support dry_run mode: validate plan without executing

### Permission Resolution (core/permissions.rs)
```rust
pub async fn effective_permission(pool: &PgPool, user_id: Uuid, app_id: Uuid) -> PermissionLevel {
    // 1. Check org role — admin = Owner everywhere
    // 2. Get direct permission from app_permissions_users (check expires_at)
    // 3. Get all team permissions from app_permissions_teams (via team_members)
    // 4. Return MAX of all
}
```

### Diagnostic Engine (core/diagnostic.rs)
```rust
pub struct ComponentDiagnosis {
    pub component_id: Uuid,
    pub health: CheckStatus,         // from check_cmd (Level 1)
    pub integrity: CheckStatus,      // from integrity_check_cmd (Level 2)
    pub infrastructure: CheckStatus, // from infra_check_cmd (Level 3)
    pub recommendation: DiagnosticRecommendation,
}

// Decision matrix:
// H=OK, I=OK, Inf=OK   → Healthy
// H=OK, I=OK, Inf=FAIL  → Healthy (warn infra)
// H=OK, I=FAIL, Inf=OK  → IntegrityWarn
// H=FAIL, I=OK, Inf=OK  → Restart
// H=FAIL, I=FAIL, Inf=OK → AppRebuild
// H=FAIL, *, Inf=FAIL    → InfraRebuild
// N/A (agent down)        → Unknown
```

### Rebuild Engine (core/rebuild.rs)
```rust
// 1. Check rebuild_protected on each target component — REJECT if protected
// 2. Resolve rebuild command: COALESCE(site_override.rebuild_cmd_override, component.rebuild_cmd)
// 3. For infra_rebuild: use rebuild_agent_id (bastion) instead of component's agent_id
// 4. Execute in DAG order (same topological sort as start)
// 5. After each component: wait check_cmd OK + integrity_check_cmd OK
// 6. On failure: SUSPEND, alert operator
// 7. Track total time as RTR (Recovery Time for Rebuild) in action_log
```

## Tests to Implement
- CRUD apps: create, read, update, delete with permission checks
- FSM: all valid transitions produce state_transitions rows
- FSM: invalid transitions return error
- DAG: cycle detection rejects circular dependencies
- DAG: topological sort produces correct level ordering
- Sequencer: 5-component app starts in correct order
- Branch: detect error branch in a 10-component graph
- Permissions: 6 levels enforce correctly (view can't start, operate can, etc.)
- Permissions: team permission + direct permission = MAX
- Permissions: expired permission ignored
- Switchover: full 6-phase cycle with rollback
- Diagnostic: all 8 matrix combinations produce correct recommendation
- Rebuild: protected component blocks rebuild
- Audit: every handler writes action_log (grep test)
