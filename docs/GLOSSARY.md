# Glossary

This page defines every term used across the AppControl documentation. Use it as a quick reference when reading the [User Guide](USER_GUIDE.md), [Architecture](architecture.md), or [Security Architecture](SECURITY_ARCHITECTURE.md).

Entries are listed alphabetically. Each entry includes a cross-reference to the page where the concept is detailed.

## Index

[A](#a) · [B](#b) · [C](#c) · [D](#d) · [E](#e) · [F](#f) · [G](#g) · [H](#h) · [I](#i) · [M](#m) · [N](#n) · [O](#o) · [P](#p) · [Q](#q) · [R](#r) · [S](#s) · [T](#t) · [U](#u) · [V](#v) · [W](#w)

---

## A

### Action log
<a id="action-log"></a>
The append-only PostgreSQL table (`action_log`) that records every user-initiated action **before** it is executed. Each row contains `user_id`, `action_type`, `target_id`, `details` (JSONB), and the timestamp. No `UPDATE` or `DELETE` is allowed — the table is the regulatory evidence required by DORA Article 16. See [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md).

### Advisory mode
<a id="advisory-mode"></a>
Operational mode in which AppControl observes components but never executes start/stop/restart. The agent reports check results normally, but the backend rejects any execute command. Used during onboarding to build confidence in the dependency map before granting operational autonomy. See [README — Garde-fous par conception](https://github.com/fredericcarre/appcontrol/blob/main/README.md).

### Agent
<a id="agent"></a>
The lightweight Rust binary (~8 MB static) installed on every monitored host. It executes health checks, integrity checks, and start/stop/rebuild commands; it sends deltas to its gateway over mTLS WebSocket. Survives crashes thanks to double-fork + setsid process detachment. See [Agent Installation](AGENT_INSTALLATION.md).

### Aggregate cluster
<a id="aggregate-cluster"></a>
Cluster mode in which the cluster is represented by a single component whose state is computed by the `check_cmd` itself (e.g. an Oracle RAC `srvctl status` script). The cluster's members are cosmetic — there is one FSM, one set of commands. Contrast with [fan-out cluster](#fan-out-cluster). Defined in migration `V035` and `V049`.

### API key
<a id="api-key"></a>
Non-interactive credential for schedulers, CI/CD pipelines, and CLI usage. Format: `ac_<random>`. The plaintext is shown once at creation; the backend stores only its SHA-256 hash in the `api_keys` table. API keys inherit the permissions of their owning user. See [Configuration — API Keys](CONFIGURATION.md#api-keys).

### Application
<a id="application"></a>
The top-level operational entity in AppControl. An application is a directed acyclic graph (DAG) of components plus metadata: name, primary site, optional DR site, tags, and per-application settings. Stored in the `applications` table. See [User Guide — Application Management](USER_GUIDE.md#application-management).

### Approval workflow
<a id="approval-workflow"></a>
The 4-eyes mechanism that requires a second approver before executing a high-risk operation (switchover, rebuild, break-glass). Configured per operation risk level. See [Security Architecture §9](SECURITY_ARCHITECTURE.md).

### Audit
<a id="audit"></a>
The collection of append-only event tables — `action_log`, `state_transitions`, `check_events`, `switchover_log`, `config_versions` — that together provide the DORA-compliant record of what happened, who did it, when, and why. See [Backup & Restore](BACKUP_RESTORE.md) for retention guidance.

### Auto-discovery
<a id="auto-discovery"></a>
The feature that lets an agent scan its host for running processes, listening ports, and service definitions, then propose a draft topology. The operator reviews and promotes the draft to a real application. See [User Guide — Auto-Discovery](USER_GUIDE.md#auto-discovery).

### Auto-PKI
<a id="auto-pki"></a>
Deployment mode in which the backend generates a per-organization CA on first startup. Agents enroll via `POST /api/v1/enroll` to obtain mTLS certificates. Eliminates manual cert handling. See [Configuration — TLS Modes](CONFIGURATION.md#three-deployment-modes).

## B

### Bastion
<a id="bastion"></a>
A privileged agent used to execute `rebuild_infra_cmd` against another host. Configured via `components.rebuild_agent_id`. The bastion is typically the only host with the credentials and network reach to reinstall an OS or restore a filesystem on the target. See [Backend CLAUDE.md — Rebuild Engine](https://github.com/fredericcarre/appcontrol/blob/main/crates/backend/CLAUDE.md).

### Binding profile
<a id="binding-profile"></a>
A named mapping of component → agent for an application, used during import and DR switchover. Examples: `prod`, `dr`, `bench`. Exactly one binding profile is active per application at any time. Stored in `binding_profiles` and `binding_profile_mappings` (migration `V030`).

### Break-glass
<a id="break-glass"></a>
Emergency access procedure using pre-provisioned accounts whose credentials are stored in an external vault. Activation triggers immediate alerts to all admins, time-limits the session (default 60 min), tags every action with `break_glass: true`, and rotates the password after use. See [Security Architecture §10](SECURITY_ARCHITECTURE.md).

## C

### CA (Certificate Authority)
<a id="ca"></a>
The root authority that signs all gateway and agent certificates. AppControl supports three setups: per-organization auto-generated CA, enterprise PKI, or cert-manager (Kubernetes). The CA private key must be backed up and protected — losing it forces re-enrollment of every component. See [Hardening](HARDENING.md).

### Check event
<a id="check-event"></a>
A single row in the `check_events` table representing one execution of a check command on one component. Includes `check_type` (`health` | `integrity` | `post_start` | `infrastructure`), `exit_code`, truncated `stdout`, and `duration_ms`. The table is partitioned by month and append-only. See [migrations/V005](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V005__event_tables.sql).

### Check type
<a id="check-type"></a>
One of the four kinds of probes AppControl runs against a component:

| Type | Frequency | Drives FSM? | Purpose |
|------|-----------|-------------|---------|
| `health` | every `check_interval_seconds` (default 30) | Yes | "Is the process alive?" |
| `integrity` | every 5 min or on-demand | No | "Is the data consistent?" |
| `post_start` | once after start | No | Initial smoke test after start |
| `infrastructure` | on-demand only | No | "Is the OS / FS / prereqs OK?" |

See also [Diagnostic levels](#diagnostic-levels).

### Cluster
<a id="cluster"></a>
A multi-host deployment of a single logical component. AppControl supports two modes:

- **[Aggregate](#aggregate-cluster)** — one FSM, one set of commands, the `check_cmd` rolls up cluster state.
- **[Fan-out](#fan-out-cluster)** — each member is a first-class monitored entity with its own agent, commands, and state.

The chosen mode is stored in `components.cluster_mode`. See migration [`V049`](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V049__fan_out_clusters.sql).

### Cluster health policy
<a id="cluster-health-policy"></a>
For fan-out clusters, the rule that derives the parent component's FSM state from its members' states. Stored in `components.cluster_health_policy` (`V049`).

| Policy | Component is `RUNNING` if … |
|--------|------------------------------|
| `all_healthy` | every member is `RUNNING` |
| `any_healthy` | at least one member is `RUNNING` |
| `quorum` | strictly more than half the members are `RUNNING` |
| `threshold_pct` | at least `cluster_min_healthy_pct` % of members are `RUNNING` |

### Component
<a id="component"></a>
A single managed process or service inside an application. Stored in the `components` table. A component has a type, an owning agent, optional commands (`check_cmd`, `start_cmd`, `stop_cmd`, `integrity_check_cmd`, `infra_check_cmd`, `rebuild_cmd`, `rebuild_infra_cmd`), timeouts, and visual position. See [Component types](#component-types).

### Component types
<a id="component-types"></a>
Built-in catalog values for `components.component_type`: `database`, `middleware`, `appserver`, `webfront`, `service`, `batch`, `custom`, `application` (the last lets a component reference another application — see [Referenced app](#referenced-app)). Migration `V031` removed the SQL `CHECK` constraint, so any string is now accepted; the [component catalog](USER_GUIDE.md#component-catalog) defines display attributes for each type.

### Config version
<a id="config-version"></a>
A snapshot of an application's configuration captured in the `config_versions` table on every change. Stores both `before` and `after` as JSONB, plus the user, timestamp, and change type. Append-only; provides DORA Article 8(2) traceability for the dependency map.

### Cross-site probe
<a id="cross-site-probe"></a>
The backend probe (every 5 min) that asks the passive site's agent to run a component's `check_cmd`. If it returns 0, the component is flagged as "running on the wrong site" — a split-brain warning. See [User Guide — Cross-Site Probe](USER_GUIDE.md#cross-site-probe).

### Custom command
<a id="custom-command"></a>
An operator-defined per-component command (log rotation, cache flush, etc.) stored in `component_commands`. Each custom command declares a `min_permission_level` (default `operate`) and an optional confirmation requirement.

## D

### DAG (Directed Acyclic Graph)
<a id="dag"></a>
The structure of an application's components and their dependencies. AppControl enforces acyclicity: import or edit operations that would introduce a cycle are rejected. The sequencer uses Kahn's algorithm to compute start/stop levels. See [Architecture — DAG Sequencing](architecture.md#data-flow-start-application-dag-sequencing).

### Delta sync
<a id="delta-sync"></a>
The agent's policy of sending a `CheckResult` only when the result changes (different `exit_code` than the last one). Reduces gateway and backend load by ~95 % on stable systems. See [Architecture — Health Check Cycle](architecture.md#data-flow-health-check-cycle).

### Dependency
<a id="dependency"></a>
A directed edge from one component to another, stored in the `dependencies` table. AppControl currently treats every dependency as a strong dependency: a parent must be `RUNNING` before its dependents start. See also [Strong / Weak dependency](#strong-dependency).

### Diagnostic levels
<a id="diagnostic-levels"></a>
The three independent probes that together describe a component's health:

| Level | Name | Command field | Drives FSM? |
|-------|------|---------------|-------------|
| 1 | Health | `check_cmd` | Yes |
| 2 | Integrity | `integrity_check_cmd` | No |
| 3 | Infrastructure | `infra_check_cmd` | No |

The diagnostic engine combines the three exit codes into a recommendation (`Healthy`, `Restart`, `AppRebuild`, `InfraRebuild`). See [Architecture — 3-Level Diagnostic + Rebuild](architecture.md#data-flow-3-level-diagnostic--rebuild).

### Discovery
<a id="discovery"></a>
See [Auto-discovery](#auto-discovery).

### DORA
<a id="dora"></a>
Regulation (EU) 2022/2554 — Digital Operational Resilience Act. Applies to financial entities and their critical ICT providers in the EU. Effective 17 January 2025. AppControl maps to Articles 8 (inventory + mapping), 11 (continuity testing), 12 (reconstruction), 16 (incident records), 25 (cyber scenarios). See [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md).

### DR (Disaster Recovery)
<a id="dr"></a>
The capability to migrate an application from its primary site to a secondary site. In AppControl, DR is a first-class operation with a 6-phase switchover engine. Not to be confused with DRaaS (cloud-hosted DR services). See [Disaster Recovery](DISASTER_RECOVERY.md).

### Dry run
<a id="dry-run"></a>
Simulation mode for any operation: validates the DAG, checks permissions and agent reachability, reports the expected execution sequence, and exits. No state change. Recommended before every switchover. See [User Guide — Dry Run](USER_GUIDE.md#dry-run-simulation).

## E

### Enrollment token
<a id="enrollment-token"></a>
A single-use (or limited-use) credential, prefixed `ac_enroll_`, presented by an agent or gateway to obtain its mTLS certificate from the backend. The backend stores only the SHA-256 hash; tokens declare a `scope` (`agent` or `gateway`), an optional `max_uses`, and a `valid_hours` window (default 24 h). See [Agent Installation §3](AGENT_INSTALLATION.md).

### Error branch
<a id="error-branch"></a>
The failed component plus all its downstream dependents. Rendered pink in the map view. The "Restart error branch" operation stops the branch (top-down), restarts the root cause, and brings up dependents (bottom-up). See [User Guide — Error Branch Restart](USER_GUIDE.md#error-branch-restart).

### Effective permission
<a id="effective-permission"></a>
The maximum of the user's direct permission on an app and the permissions granted via every team they belong to. Computed by `core::permissions::effective_permission()` and refreshed on every API call. See [Permissions](PERMISSIONS.md).

## F

### Fan-out cluster
<a id="fan-out-cluster"></a>
Cluster mode in which each member (host, instance) is a first-class entity with its own agent assignment, its own override commands, and its own FSM state. The parent component's state is derived from members' states by the [cluster health policy](#cluster-health-policy). Defined in `cluster_members` and `cluster_member_state` (migration `V049`).

### FSM (Finite State Machine)
<a id="fsm"></a>
The state model that drives every component. States: `UNKNOWN`, `RUNNING`, `STOPPED`, `FAILED`, `DEGRADED`, `STARTING`, `STOPPING`, `UNREACHABLE`. Transitions are validated by `common::fsm::is_valid_transition()` (`crates/common/src/fsm.rs`). Every transition is logged to `state_transitions`. See [Architecture — FSM State Machine](architecture.md#fsm-state-machine).

## G

### Gateway
<a id="gateway"></a>
A stateless WebSocket relay that sits between agents and the backend. Each network zone (PRD, DR, DMZ) typically has its own gateway. The gateway terminates mTLS from agents, forwards traffic to the backend, holds no database connection, and exposes `POST /enroll` for token-based agent enrollment. See [Configuration — Gateway](CONFIGURATION.md#gateway).

## H

### Heartbeat
<a id="heartbeat"></a>
The 60-second-interval message an agent sends to the gateway containing CPU, memory, and system info. Missing 3 consecutive heartbeats (default 180 s — `organizations.heartbeat_timeout_seconds`) marks the agent stale; the heartbeat monitor task transitions its managed components to `UNREACHABLE`. See [Backend CLAUDE.md — Heartbeat Monitor](https://github.com/fredericcarre/appcontrol/blob/main/crates/backend/CLAUDE.md).

### High availability (HA)
<a id="ha"></a>
Deployment mode where the backend runs as multiple replicas behind a load balancer with `HA_MODE=true` (rate limiting moves to PostgreSQL). Gateways are stateless; agents implement client-side failover across multiple gateways. See [High Availability](HIGH_AVAILABILITY.md).

### Hosting
<a id="hosting"></a>
A logical grouping of sites by physical datacenter or cloud region. Example: hosting "DC Paris" contains sites `prod-paris` and `staging-paris`. During switchover, the UI distinguishes intra-hosting from cross-hosting failovers. Stored in the `hostings` table (migration `V046`). See [User Guide — Hostings](USER_GUIDE.md#hostings).

## I

### Infrastructure check
<a id="infrastructure-check"></a>
Level-3 diagnostic probe (`infra_check_cmd`). On-demand only; informational. Verifies that the underlying OS, filesystem, listener ports, and prereqs are in order before a rebuild. See [Diagnostic levels](#diagnostic-levels).

### Integrity check
<a id="integrity-check"></a>
Level-2 diagnostic probe (`integrity_check_cmd`). Runs every 5 minutes or on-demand. Verifies data consistency (e.g. transaction log alignment, replica lag). Does **not** drive the FSM. See [Diagnostic levels](#diagnostic-levels).

## M

### MCP (Model Context Protocol)
<a id="mcp"></a>
A standardized protocol for connecting AI agents (Claude, ChatGPT, Cursor, …) to external tools. AppControl exposes a native MCP server with 7 tools (status, diagnose, start/stop/restart, switchover, rebuild). The MCP server runs as a sub-module of the backend, available over stdio or HTTP. See `crates/backend/src/mcp/`.

### mTLS
<a id="mtls"></a>
Mutual TLS — both endpoints present X.509 certificates. AppControl requires mTLS between agent and gateway. The CA, gateway, and agent certificates are managed by the [PKI](#pki) layer. The agent's certificate `CN` must match its `agent_id` UUID derivation (verified by the gateway). See [Security Architecture §3](SECURITY_ARCHITECTURE.md).

## N

### Native command
<a id="native-command"></a>
A typed JSON payload alternative to a shell `check_cmd` / `start_cmd` / `stop_cmd`. Useful on hosts where shell utilities (`curl`, `wget`) are not installed. Currently supported kinds:

- `http` — HTTP probe with method, URL, expected status, headers, optional body.
- `tcp` — TCP connect to host:port (planned, same column).
- `process` — process-name match (planned, same column).

Stored in `components.check_native` / `start_native` / `stop_native` (JSONB). When non-null, the agent uses the native spec instead of the shell command. See migration [`V053`](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V053__native_command_specs.sql).

### NIS2
<a id="nis2"></a>
Directive (EU) 2022/2555 — Network and Information Security 2. Applies to essential and important entities across critical sectors. AppControl supports Article 21 (risk-mitigation measures) through RBAC, audit, BCM, mTLS, and the hardening checklist. See [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md).

## O

### OIDC
<a id="oidc"></a>
OpenID Connect — the SSO mechanism AppControl uses for browser logins. Tested with Keycloak, Okta, Azure AD, Google Workspace. Configured via `OIDC_DISCOVERY_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`. See [Configuration — OIDC](CONFIGURATION.md#oidc-configuration).

### Organization
<a id="organization"></a>
The top-level tenant in the AppControl data model. Every user, application, agent, and gateway belongs to exactly one organization. Stored in the `organizations` table. Most installs run a single organization; multi-tenancy is supported by isolating data via `organization_id`.

### Org role
<a id="org-role"></a>
A user's platform-wide role: `admin`, `operator`, `editor`, or `viewer`. Stored in `users.role`. The org role controls admin features (manage users, teams, sites). It does **not** restrict per-application access — a `viewer` with `operate` permission on an app can still start it. See [Permissions — Platform Roles](PERMISSIONS.md#platform-roles).

## P

### PKI
<a id="pki"></a>
The certificate authority and certificate-issuance flow used for mTLS. AppControl supports per-organization PKI (auto-initialized), enterprise PKI (bring-your-own CA), and cert-manager (Kubernetes). See [Configuration — TLS](CONFIGURATION.md#tls--mtls-certificate-configuration).

### Permission level
<a id="permission-level"></a>
The granular access level a user (or team) has on a specific application:

```
view < operate < edit < manage < owner
```

| Level | Numeric | Capability |
|-------|--------:|------------|
| `view` | 1 | Read map, status, logs |
| `operate` | 2 | Start, stop, restart, run diagnostics |
| `edit` | 3 | Modify components, commands, dependencies |
| `manage` | 4 | Grant permissions, share links |
| `owner` | 5 | Delete app, transfer ownership |

The effective level is `MAX(direct, all_team_grants)`. Org admins are implicit owners on everything. See [Permissions](PERMISSIONS.md).

### Pink branch
<a id="pink-branch"></a>
See [Error branch](#error-branch). The visual color used to highlight failed components and their dependents in the map view.

### PR-only mode
<a id="pr-only-mode"></a>
Optional mode in which start/stop operations are gated behind a merged Pull Request. The map (components, commands, dependencies) is versioned as code and operations are bound to git history. Used in highly regulated environments. See [README — Garde-fous par conception](https://github.com/fredericcarre/appcontrol/blob/main/README.md).

## Q

### Quorum
<a id="quorum"></a>
[Cluster health policy](#cluster-health-policy) in which the parent component is `RUNNING` if strictly more than half its enabled members are `RUNNING`.

## R

### Rebuild
<a id="rebuild"></a>
The targeted reconstruction operation that follows a diagnostic. Two flavors:

- **App rebuild** — uses `rebuild_cmd`; reinstalls the application layer, keeps the host.
- **Infra rebuild** — uses `rebuild_infra_cmd`; reinstalls the infrastructure (OS, filesystem). Executed by a [bastion](#bastion) agent (`rebuild_agent_id`).

Rebuild respects DAG order and skips components flagged with `rebuild_protected = true`. See [Architecture — 3-Level Diagnostic + Rebuild](architecture.md#data-flow-3-level-diagnostic--rebuild).

### Rebuild protection
<a id="rebuild-protection"></a>
The `rebuild_protected = true` flag on a component prevents the rebuild engine from touching it. Used for components whose data must never be regenerated by AppControl (databases of record, archive storage). A rebuild against a protected component returns HTTP 409. See [Backend CLAUDE.md — Rebuild Engine](https://github.com/fredericcarre/appcontrol/blob/main/crates/backend/CLAUDE.md).

### Referenced app
<a id="referenced-app"></a>
A component of type `application` with a `referenced_app_id` pointing to another application. When the parent app starts, the referenced app is recursively started first; its state is derived from its components' aggregate state, not its own cached column. See [Backend CLAUDE.md — Application-Type Components](https://github.com/fredericcarre/appcontrol/blob/main/crates/backend/CLAUDE.md).

### Retention
<a id="retention"></a>
The configurable lifetime of audit rows. Two knobs:

- `RETENTION_ACTION_LOG_DAYS` — default `0` (unlimited). Deletes `action_log` rows older than N days.
- `RETENTION_CHECK_EVENTS_DAYS` — default `0` (unlimited). Drops `check_events` partitions older than N days.

DORA requires 5 years minimum for incident records. See [Hardening](HARDENING.md).

### RPO (Recovery Point Objective)
<a id="rpo"></a>
The maximum acceptable data loss in time. For AppControl audit data, the RPO depends on your PostgreSQL backup cadence: typical RPO is 5–15 minutes if WAL archiving is configured, 1–24 hours otherwise. See [Disaster Recovery](DISASTER_RECOVERY.md).

### RTO (Recovery Time Objective)
<a id="rto"></a>
The maximum acceptable downtime. For AppControl-managed applications, the RTO is measured end-to-end through `switchover_log` (time from FREEZE to COMMIT phase). See [Disaster Recovery](DISASTER_RECOVERY.md).

### RTR (Recovery Time for Rebuild)
<a id="rtr"></a>
The total elapsed time of a rebuild operation, measured by the rebuild engine and stored in `action_log`. Tracked over time to identify regression in reconstruction speed (DORA Article 11). See [Architecture — 3-Level Diagnostic + Rebuild](architecture.md#data-flow-3-level-diagnostic--rebuild).

## S

### SAML
<a id="saml"></a>
Security Assertion Markup Language 2.0 — alternative SSO mechanism, tested with ADFS, Azure AD, Okta. AppControl supports group-to-team auto-sync via `saml_group_mappings`. Configured via `SAML_IDP_SSO_URL`, `SAML_IDP_CERT`, `SAML_SP_ENTITY_ID`. See [Configuration — SAML](CONFIGURATION.md#saml-20-configuration).

### Scheduler integration
<a id="scheduler-integration"></a>
The REST API and `appctl` CLI exposed for use by enterprise schedulers (Control-M, AutoSys, Dollar Universe, TWS). Standard exit codes (`0` success, `1` failure, `2` timeout, `3` auth, `4` not found, `5` permission denied) make integration straightforward. AppControl **is not** itself a scheduler. See [Configuration — CLI](CONFIGURATION.md#cli-appctl).

### Site
<a id="site"></a>
A physical or logical location: a datacenter, a DR site, a staging environment. Stored in the `sites` table. Every application and gateway belongs to a site. Sites are grouped by [hosting](#hosting). See [User Guide — Sites](USER_GUIDE.md#sites).

### Site override
<a id="site-override"></a>
Per-site customization of a component, stored in `site_overrides`. Lets the same component run on PRD and DR with different `start_cmd`, `agent_id`, `env_vars`, etc. The active site at any time is determined by `applications.active_site_id`.

### State transition
<a id="state-transition"></a>
A row in the append-only `state_transitions` table recording a change of a component's FSM state. Includes `previous_state`, `next_state`, `trigger` (e.g. `health_check`, `sequencer`, `heartbeat_timeout`), and a JSON `details` blob. Used for DORA Article 16 audit. See [migrations/V005](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V005__event_tables.sql).

### Strong dependency
<a id="strong-dependency"></a>
A dependency that gates start/stop sequencing. The parent must reach `RUNNING` before the dependent starts; the dependent must reach `STOPPED` before the parent stops. AppControl currently treats every edge in the `dependencies` table as a strong dependency. The "weak dependency" concept is discussed in the user-guide narrative but does not yet have a dedicated column.

### Switchover
<a id="switchover"></a>
The DR operation that migrates an application from its current site to another site. Executed in six phases (see [Switchover phases](#switchover-phases)). Tracked end-to-end in `switchover_log`. See [User Guide — DR Switchover](USER_GUIDE.md#dr-site-switchover).

### Switchover phases
<a id="switchover-phases"></a>
The six phases of a switchover:

| Phase | Name | Description |
|------:|------|-------------|
| 1 | Prepare | Verify DR agents connected and resources available |
| 2 | Validate | Run pre-flight health checks on the target site |
| 3 | StopSource | Stop all components on the source site (reverse DAG) |
| 4 | Sync | Verify data replication is complete |
| 5 | StartTarget | Start all components on the target site (DAG order) |
| 6 | Commit | Update `applications.active_site_id` — point of no return |

Rollback is available up to phase 5. See [Disaster Recovery](DISASTER_RECOVERY.md).

### Switchover mode
<a id="switchover-mode"></a>
The flavor of switchover:

- `FULL` — every component is migrated.
- `SELECTIVE` — only operator-chosen components are migrated.
- `PROGRESSIVE` — components are migrated in DAG levels, with a pause between each level for validation.

Stored in `switchover_log.mode`.

## T

### Team
<a id="team"></a>
A named group of users in the `teams` table. Teams are granted application permissions; members inherit those grants. Teams can be auto-synced from SAML groups via `saml_group_mappings`. See [Permissions — Teams](PERMISSIONS.md).

### TcpConnect
<a id="tcpconnect"></a>
A [native command](#native-command) `kind` that attempts a TCP connect to a host:port. Used for simple liveness probes when no shell utility is available on the host. The agent reports exit code 0 on successful connect, non-zero otherwise.

## U

### UNREACHABLE
<a id="unreachable"></a>
The FSM state assigned to a component when its agent fails to send 3 consecutive heartbeats. The component's previous state is preserved in `state_transitions.details.previous_state` so the FSM can restore it on reconnect. See [Backend CLAUDE.md — Heartbeat Monitor](https://github.com/fredericcarre/appcontrol/blob/main/crates/backend/CLAUDE.md).

## V

### Vault
<a id="vault"></a>
External credential store (HashiCorp Vault, AWS KMS, Azure Key Vault) that holds secret variable values. AppControl never sees the plaintext: it sends a placeholder (`$(secret:NAME)`) in the command, the agent resolves it from the vault at execution time, and zeroes the memory after exec. See [Security Architecture §11](SECURITY_ARCHITECTURE.md).

## W

### Weak dependency
<a id="weak-dependency"></a>
A dependency edge that is **not** gated by the DAG sequencer: both endpoints start in parallel, but the edge is still drawn in the map and exported in reports. Useful for documenting "B reads from A, but B must not block on A at start-up". Stored as `dependencies.dependency_type = 'weak'` (introduced in migration V056) and exposed via the `POST /api/v1/apps/:id/dependencies` API as `{"dependency_type": "weak"}`. Defaults to [strong](#strong-dependency) so pre-V056 rows and older clients see no behaviour change.

### WebSocket hub
<a id="websocket-hub"></a>
The backend component that manages per-user WebSocket subscriptions. On every `Subscribe(app_id)`, the hub computes the user's effective permission and rejects the subscription if it is below `view`. State change events are fan-out only to subscribers who pass the permission filter. See [Security Architecture §7](SECURITY_ARCHITECTURE.md).

### Workspace
<a id="workspace"></a>
A site-scoped access boundary. A workspace binds users (directly or via teams) to a set of sites. When workspaces are configured, a user can only access components whose site is included in at least one of their workspaces. Without workspaces, all sites are accessible (the feature is opt-in). Defined in `workspace_sites` and `workspace_members` (migration `V011`).

---

*Last updated alongside the codebase. To report a missing or stale entry, open a documentation issue.*
