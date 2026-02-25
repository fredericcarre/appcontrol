# AppControl v4 User Guide

## Overview

AppControl is an enterprise platform for **operational mastery and IT system resilience**. It maps applications as dependency graphs (DAGs), monitors component health via distributed agents, orchestrates sequenced start/stop/restart operations, manages DR site failover, and provides full DORA-compliant audit trails.

AppControl is **NOT a scheduler**. It integrates with existing schedulers (Control-M, AutoSys, Dollar Universe, TWS) via REST API and CLI.

---

## Core Concepts

### Applications & Components

An **Application** in AppControl is a collection of components that form a directed acyclic graph (DAG). Each application represents a business service or system composed of interdependent processes, services, and resources spread across one or more servers.

A **Component** is an individual process or service running on a monitored server. Components are the atomic units of management within AppControl. Each component has:

- A **check command** that determines its health status
- Optional **start** and **stop** commands for orchestrated operations
- **Dependencies** on other components within the same application
- A **finite state machine (FSM)** that tracks its current operational state

### FSM States

Every component in AppControl is tracked by a finite state machine with the following states:

| State | Description |
|-------|-------------|
| `UNKNOWN` | Initial state before the first health check completes. The component has been defined but its status has not yet been determined. |
| `RUNNING` | The component is healthy and operating normally. The check command returns a success exit code. |
| `STOPPED` | The component is intentionally stopped. This is a normal, expected state after a stop operation. |
| `FAILED` | The component has encountered an error. The check command returns a failure exit code when the component was previously running. |
| `DEGRADED` | The component is running but not at full capacity. Some sub-checks pass while others fail, or dependent services are impaired. |
| `STARTING` | A start command has been issued and is currently executing. This is a transient state. |
| `STOPPING` | A stop command has been issued and is currently executing. This is a transient state. |
| `UNREACHABLE` | The agent monitoring this component has lost connectivity. The component may still be running, but its status cannot be confirmed. |

State transitions are recorded in the `state_transitions` table for full auditability. Every transition is traced and timestamped.

### Dependency Graph (DAG)

Applications are modeled as directed acyclic graphs where edges represent dependencies between components.

- **Start order:** bottom-up. Components with no dependencies start first, then components that depend on them start next, and so on up the graph. A component will not start until all of its dependencies are in a `RUNNING` state.
- **Stop order:** top-down (reverse). Components at the top of the graph (those that depend on others) stop first. A component will not stop until all components that depend on it have reached a `STOPPED` state.
- **Parallel execution:** Components at the same level in the DAG (with no dependencies between them) are started or stopped in parallel for maximum efficiency.

The DAG is visualized interactively in the Map View using React Flow, allowing operators to see the full topology of an application at a glance.

### Diagnostic Levels

AppControl provides a three-level diagnostic framework for progressively deeper assessment of component health:

| Level | Name | Command | Frequency | Purpose |
|-------|------|---------|-----------|---------|
| **Level 1** | Health | `check_cmd` | Every 30 seconds | Drives the FSM. Answers: "Is the process alive?" This is the primary health check that determines component state. |
| **Level 2** | Integrity | `integrity_check_cmd` | Every 5 minutes or on-demand | Informational only (does not affect FSM). Answers: "Is the data consistent?" Checks internal data integrity, configuration validity, and logical correctness. |
| **Level 3** | Infrastructure | `infra_check_cmd` | On-demand only | Informational only (does not affect FSM). Answers: "Is the OS/filesystem/prerequisites OK?" Checks disk space, memory, network connectivity, certificate expiry, and other infrastructure prerequisites. |

Diagnostic results are stored and can be used to drive the **Diagnostic & Rebuild** workflow, which performs a surgical reconstruction of failed components based on the assessment results.

---

## Features

### Dashboard

The Dashboard provides a real-time overview of all applications the current user has access to. It serves as the primary landing page after login.

**Key elements:**

- **Application cards** showing each application's name, overall status, and component summary
- **KPI tiles** displaying aggregate metrics: total applications, components running, components failed, agents connected
- **Event feed** showing the most recent operations, state changes, and alerts across all visible applications
- **Quick filters** to focus on applications by status (healthy, degraded, failed, unknown) or by site/zone
- **Search** to quickly locate applications by name or tag

The Dashboard updates in real time via WebSocket connections, so status changes appear immediately without manual refresh.

<!-- SCREENSHOT:dashboard -->
*Dashboard — application overview with real-time KPIs*

### Map View

The Map View provides an interactive DAG visualization of a single application's component topology. It is the primary interface for understanding dependencies and performing operations.

**Capabilities:**

- **Interactive graph** rendered with React Flow, supporting pan, zoom, and drag
- **Color-coded nodes** reflecting each component's current FSM state (green for running, red for failed, gray for stopped, etc.)
- **Click on a node** to open the component detail panel showing:
  - Current state and state history
  - Recent check results (all three diagnostic levels)
  - Start/stop/custom command outputs
  - Log tail from the agent
  - Component configuration
- **Edge visualization** showing dependency relationships with directional arrows
- **Animated transitions** when components change state during operations
- **Layout controls** for automatic graph arrangement (top-down, left-right, radial)
- **Error branch highlighting** (pink branch) to visually identify the chain of components affected by a failure
- **Mini-map** for navigation in large application graphs

<!-- SCREENSHOT:map-view -->
*Map View — interactive DAG visualization*

### Operations

AppControl supports seven core operation types. Every operation is logged in the `action_log` table **before** execution begins, ensuring a complete audit trail even if the operation fails partway through.

#### 1. Full Application Start (DAG Sequencing)

Starts all components in the application following the dependency graph from bottom to top.

- Components with no dependencies start first
- Each component waits for its dependencies to reach `RUNNING` before starting
- Components at the same dependency level start in parallel
- The operation tracks progress and reports per-component status
- If a component fails to start, dependent components are not attempted (fail-fast behavior, configurable)

#### 2. Full Application Stop (Reverse DAG)

Stops all components in reverse dependency order (top to bottom).

- Components with no dependents stop first
- Each component waits for all components that depend on it to reach `STOPPED` before stopping
- Parallel execution at each level for efficiency
- Configurable timeout per component; if a component does not stop gracefully within the timeout, a force-stop can be issued

#### 3. Error Branch Restart (Pink Branch)

When a component fails, AppControl identifies the **error branch**: the failed component and all components that depend on it (directly or transitively). These are highlighted in pink on the Map View.

The error branch restart:
1. Stops all components in the error branch (top-down)
2. Restarts the failed component
3. Starts all dependent components (bottom-up) once the root cause component is running

This targeted approach avoids restarting the entire application when only a subset is affected.

#### 4. DR Site Switchover (6 Phases)

Disaster Recovery switchover migrates an application from the primary site to the DR site. The process follows six phases:

| Phase | Name | Description |
|-------|------|-------------|
| 1 | **Pre-check** | Validate DR site readiness: agents connected, resources available, prerequisites met |
| 2 | **Quiesce** | Gracefully stop incoming traffic and drain active sessions on the primary site |
| 3 | **Stop Primary** | Stop all components on the primary site following reverse DAG order |
| 4 | **Data Sync** | Ensure data replication is complete and consistent between sites |
| 5 | **Start DR** | Start all components on the DR site following DAG order |
| 6 | **Validate** | Run health checks on the DR site to confirm successful switchover |

Each phase is logged and can be monitored in real time. The switchover can be paused, resumed, or rolled back (phases 1-3 only) if issues are detected.

Switchover events are recorded in the `switchover_log` table for DORA compliance reporting.

#### 5. Custom Commands

Custom commands allow operators to execute predefined commands on specific components. These are configured per component in the application definition and can include:

- Log rotation
- Cache clearing
- Configuration reload
- Database maintenance tasks
- Any other operational command

Custom commands are subject to the same permission checks as start/stop operations and are fully logged in the audit trail.

#### 6. Dry Run Simulation

Before executing any operation, operators can perform a dry run that simulates the operation without making changes. The dry run:

- Validates the DAG and dependency order
- Checks that all required agents are connected and responsive
- Verifies that the current user has sufficient permissions for all components involved
- Reports the expected execution sequence and estimated duration
- Identifies potential issues (unreachable agents, permission gaps, missing commands)

Dry runs are recommended before performing switchovers or large-scale operations in production environments.

#### 7. Diagnostic & Rebuild (3-Level Assessment)

The Diagnostic & Rebuild workflow combines all three diagnostic levels into a comprehensive assessment, then performs surgical reconstruction of failed components:

1. **Level 1 (Health):** Identify which components are not running
2. **Level 2 (Integrity):** Check data consistency for failed components
3. **Level 3 (Infrastructure):** Verify OS/filesystem/prerequisites for the affected servers
4. **Assessment report:** Present findings with recommended actions
5. **Rebuild execution:** Apply targeted fixes based on the assessment (restart services, repair data, resolve infrastructure issues)

This workflow is designed for complex failure scenarios where a simple restart is insufficient.

### Sharing & Permissions

AppControl provides a granular permission model that controls access to applications at the user and team level.

#### Permission Levels

Permissions are hierarchical. Each level includes all capabilities of the levels below it:

| Level | Capabilities |
|-------|-------------|
| `view` | View application status, map, component details, logs, and reports |
| `operate` | All of `view` plus: start, stop, restart operations, execute custom commands |
| `edit` | All of `operate` plus: modify application configuration, add/remove components, edit dependencies |
| `manage` | All of `edit` plus: share the application with other users/teams, manage permissions |
| `owner` | All of `manage` plus: delete the application, transfer ownership |

**Effective permission** = MAX(direct_user_permission, team_permissions). If a user has `view` permission directly but belongs to a team with `operate` permission, the effective permission is `operate`.

**Organization admins** have implicit `owner` permission on all applications in the organization.

#### Sharing Applications

Applications can be shared with other users and teams through the Share Modal:

- **User search with autocomplete:** Type a username or email to find users in the organization. The autocomplete searches across all users the current user can see.
- **Direct user permission grant:** Select a user and assign a permission level. The permission takes effect immediately.
- **Team permission grant:** Select a team and assign a permission level. All current and future members of that team inherit the permission.
- **Bulk operations:** Share with multiple users or teams at once.

<!-- SCREENSHOT:share-modal -->
*Share Modal — user picker and share link management*

#### Share Links

Share links provide a convenient way to grant access to applications, similar to sharing documents in Google Docs.

- **Create a share link** from the Share Modal with a specified permission level
- **Time-limited:** Set an expiration date after which the link no longer works
- **Use-limited:** Set a maximum number of uses (e.g., allow only 5 people to claim the link)
- **Revocable:** Disable a share link at any time; existing permissions granted through the link remain unless explicitly revoked
- **Audited:** Every use of a share link is recorded in the audit trail

### API Keys

API keys enable programmatic access to AppControl for scheduler integration, automation scripts, and CI/CD pipelines.

**Creating API keys:**
1. Navigate to **Settings > API Keys**
2. Click **Create API Key**
3. Provide a descriptive name (e.g., "Control-M Production")
4. Optionally configure **scopes** to restrict the key to specific actions (e.g., only start/stop operations on specific applications)
5. Copy the generated key immediately; it will not be shown again

**Key format:** `ac_<uuid>` (e.g., `ac_550e8400-e29b-41d4-a716-446655440000`)

**Security:**
- Keys are stored as **SHA-256 hashes** in the database; the plaintext key is never stored
- Keys can be revoked at any time from the API Keys management page
- All API key usage is logged in the audit trail with the key's name for traceability
- Keys inherit the permissions of the user who created them

<!-- SCREENSHOT:api-keys -->
*API Keys — key management page*

### Agent Management

Agents are lightweight processes deployed on monitored servers. They execute health checks, start/stop commands, and report status back to the platform via the gateway.

**Agent monitoring:**
- View all connected agents and their status (connected, disconnected, degraded)
- See heartbeat timestamps and latency
- Monitor agent version and capabilities
- View the list of components managed by each agent

**Agent communication:**
- Agents communicate through the gateway using mTLS (mutual TLS) for security
- Communication uses delta-only sync: agents send only state changes, not full status on every check cycle
- Heartbeat interval is configurable (default: 30 seconds)

<!-- SCREENSHOT:agents -->
*Agents — connected agent monitoring*

#### Agent Enrollment

New agents are onboarded using enrollment tokens for secure, authenticated registration.

**Enrollment workflow:**
1. An administrator generates an **enrollment token** from the Agent Management page
2. The token is provided to the agent during installation (via CLI flag or configuration file)
3. The agent presents the token to the gateway on first connection
4. The gateway validates the token, issues mTLS certificates, and registers the agent
5. The enrollment token is single-use and expires after a configurable duration (default: 24 hours)

This process ensures that only authorized agents can join the platform, preventing unauthorized access.

### Team Management

Teams provide a way to group users and assign permissions collectively.

**Creating and managing teams:**
- Create teams with descriptive names (e.g., "Database Operations", "Night Shift Operators")
- Add and remove team members
- Assign team-wide permissions on applications (all members inherit the permission)
- View team membership and permission summary

**Team permissions** are combined with direct user permissions using the MAX function, ensuring that team membership never reduces a user's access level.

### Workspace / Zone Access Control

Workspaces (also called zones) provide a way to segment the platform by site, environment, or organizational boundary.

- **Restrict visibility:** Users and teams can be assigned to specific workspaces, limiting which applications and agents they can see
- **Multi-site support:** Map workspaces to physical sites (e.g., "Paris DC", "London DR") for DR operations
- **Environment isolation:** Separate production, staging, and development environments
- **Cross-workspace operations:** Switchover operations can span workspaces (e.g., failing over from the Paris workspace to the London workspace)

### Reports (DORA Compliance)

AppControl provides comprehensive reporting capabilities aligned with DORA (Digital Operational Resilience Act) requirements.

**Available reports:**

| Report | Description |
|--------|-------------|
| **Availability** | Uptime and downtime metrics per application and component over configurable time periods |
| **Incidents** | Incident timeline showing failures, root causes, resolution times, and impacted components |
| **Switchover History** | Complete history of DR switchovers with phase-by-phase timing and outcomes |
| **Audit Trail** | Full `action_log` showing every user action with timestamps, user identity, and action details |
| **RTO Analysis** | Recovery Time Objective analysis comparing actual recovery times against configured targets |

**Report features:**
- Configurable date ranges and filters
- Drill-down from summary to component-level detail
- **PDF export** for offline review and regulatory submission
- Scheduled report generation (daily, weekly, monthly)
- Dashboard widgets for key metrics

<!-- SCREENSHOT:reports -->
*Reports — DORA compliance metrics*

### Scheduler Integration

AppControl integrates with enterprise schedulers via REST API and CLI. It is designed to be called by schedulers, not to replace them.

**Supported schedulers:**
- Control-M
- AutoSys
- Dollar Universe
- TWS (Tivoli Workload Scheduler)

**Integration patterns:**
- **REST API:** Schedulers call AppControl's API to trigger start/stop operations, check status, and retrieve results. Authentication is via API keys.
- **CLI (`appctl`):** The `appctl` command-line tool can be invoked from scheduler job definitions. It supports all operations and returns appropriate exit codes for scheduler integration.

**Example CLI usage:**
```bash
# Start an application and wait for completion
appctl app start --name "Trading Platform" --wait --timeout 300

# Check application status
appctl app status --name "Trading Platform" --format json

# Stop a specific component
appctl component stop --app "Trading Platform" --component "Order Service"
```

### YAML Import

Application maps can be defined in YAML files and imported into AppControl. This supports infrastructure-as-code workflows and version-controlled application definitions.

**YAML import workflow:**
1. Define the application structure in a YAML file (components, dependencies, commands, check intervals)
2. Upload the YAML file through the UI or via the CLI (`appctl app import --file app.yaml`)
3. AppControl validates the YAML against the schema (checks for cycles in the DAG, missing references, etc.)
4. The application is created or updated based on the YAML definition
5. The imported configuration is stored as a version in `config_versions` with full before/after comparison

### Approval Workflows (4-Eyes Principle)

For critical operations in regulated environments, AppControl supports approval workflows that enforce the four-eyes principle.

**Configuration:**
- Define **approval policies** specifying which operations require approval (e.g., production stop, switchover)
- Set the **number of required approvals** (e.g., 2 approvals for switchover)
- Configure **eligible approvers** (specific users, team members, or role-based)
- Set **approval timeout** after which the request expires

**Workflow:**
1. An operator initiates an operation that requires approval
2. Designated approvers receive a notification (in-app, email, or webhook)
3. Approvers review the operation details and approve or reject
4. Once the required number of approvals is reached, the operation executes automatically
5. All approval decisions are recorded in the audit trail

### Break-Glass Emergency Access

In critical emergency situations, operators can invoke break-glass access to bypass normal permission checks and approval workflows.

**How it works:**
1. The operator requests break-glass access, providing a justification
2. Access is granted immediately (no approval required)
3. A **high-priority alert** is sent to all administrators
4. The operator can perform any operation for a limited time window (configurable, default: 30 minutes)
5. Every action taken during the break-glass session is logged with a special flag in the audit trail
6. A mandatory **post-incident review** is required to close the break-glass session

Break-glass access ensures that critical operations are never blocked by permission issues while maintaining full accountability.

---

## Administration

### Admin Hierarchy

AppControl distinguishes two levels of administration:

| Level | Role | Scope | Can Do |
|-------|------|-------|--------|
| **Platform Super-Admin** | `platform_role = 'super_admin'` | All organizations | Create/manage organizations, assign initial org admins |
| **Org Admin** | `role = 'admin'` | Single organization | Manage sites, users, teams, tokens, certs, apps |
| **Operator** | `role = 'operator'` | Per-app permission | Start, stop, restart applications |
| **Editor** | `role = 'editor'` | Per-app permission | Modify components, dependencies, commands |
| **Viewer** | `role = 'viewer'` | Per-app permission | Read-only access to maps, logs, reports |

**How users are created:**

| Method | Who Creates | When |
|--------|-------------|------|
| **Dev seed** | Auto | Backend startup in dev mode (`admin@localhost`, super-admin) |
| **OIDC/SAML** | Auto | First login via identity provider |
| **API** | Org admin | `POST /api/v1/users` (local users) |
| **Super-admin** | Super-admin | `POST /api/v1/organizations` creates org + initial admin |

### Initial Setup Workflow

```
1. Super-admin logs in
2. Super-admin creates organizations (POST /api/v1/organizations)
   → Each org gets an initial admin + auto-generated PKI (CA)
3. Org admin logs in to their organization
4. Org admin creates sites (POST /api/v1/sites)
5. Org admin creates gateway enrollment tokens (scope: "gateway")
6. Deploy & enroll gateways → they register in gateways table
7. Org admin assigns gateways to sites (PUT /api/v1/gateways/:id)
8. Org admin creates agent enrollment tokens (scope: "agent")
9. Deploy & enroll agents → they register in agents table
10. Org admin creates applications with components and dependencies
11. Org admin configures teams, permissions, workspaces
12. Org admin sets up SAML group mappings (for auto-provisioning)
13. Org admin creates API keys (for scheduler integration)
```

**Default dev account:**

| Field | Value |
|-------|-------|
| Email | `admin@localhost` |
| Org Role | `admin` |
| Platform Role | `super_admin` |
| Organization | `Dev Org` (ID: `00000000-0000-0000-0000-000000000001`) |

In production, the first super-admin is bootstrapped via OIDC/SAML + database: `UPDATE users SET platform_role = 'super_admin' WHERE email = 'first-admin@corp.com'`.

### Sites

Sites represent physical or logical locations: datacenters, DR sites, staging environments. Every application and gateway belongs to a site.

**Site types:** `primary` (production datacenter), `dr` (disaster recovery), `staging` (pre-production), `development`.

**API endpoints:**
- `GET /api/v1/sites` — List sites (filterable by `site_type`, `is_active`)
- `POST /api/v1/sites` — Create site (admin only)
- `GET /api/v1/sites/:id` — Get site details
- `PUT /api/v1/sites/:id` — Update site (admin only)
- `DELETE /api/v1/sites/:id` — Delete site (fails if applications are linked)

Sites are created at setup time and bound to workspaces for access control. During DR switchover, AppControl orchestrates the transition from a primary site to a DR site.

### Gateway & Agent Enrollment

Gateways and agents authenticate via mTLS certificates issued during enrollment. The enrollment flow uses one-time tokens created by administrators.

**Enrollment token scopes:**
- `gateway` — Enroll gateways (which accept agent connections)
- `agent` — Enroll agents (which execute commands on servers)

**Gateway enrollment:** Admin creates token (`scope: "gateway"`) → deploy gateway binary on site → start with `--enrollment-token` → gateway calls `POST /api/v1/enroll` → receives mTLS cert signed by org CA → connects to backend.

**Agent enrollment:** Admin creates token (`scope: "agent"`) → deploy agent binary on server → start with `--enrollment-token` and `--gateway-url` → agent calls `POST /api/v1/enroll` → receives mTLS cert → connects to gateway.

**Token management (API):**
- `POST /api/v1/enrollment/tokens` — Create token (admin only)
- `GET /api/v1/enrollment/tokens` — List tokens with usage stats
- `POST /api/v1/enrollment/tokens/:id/revoke` — Revoke a token
- `GET /api/v1/enrollment/events` — Audit trail of all enrollment attempts

**Token management (CLI):**
```bash
appctl pki create-token --name "gw-paris" --scope gateway --max-uses 3 --valid-hours 48
appctl pki create-token --name "agents-paris" --scope agent --max-uses 100 --valid-hours 720
appctl pki list-tokens
appctl pki revoke-token <token-id>
```

### Certificate Security (Anti-Spoofing)

AppControl prevents gateway and agent impersonation through five security layers:

1. **Token-based enrollment** — SHA-256 hashed, scoped (`gateway`/`agent`), expirable, revocable, max-usage-limited
2. **mTLS (Mutual TLS)** — Per-organization CA, certs signed during enrollment, all connections encrypted and mutually authenticated
3. **Certificate pinning** — SHA-256 fingerprint stored at enrollment, verified on every connection. A valid cert from a different agent is rejected
4. **Certificate revocation** — Admins revoke certs via API (`POST /agents/:id/revoke-cert`). Revoked fingerprints are checked in real-time. Revocation deactivates the agent/gateway immediately
5. **Audit trail** — All enrollment attempts, cert issuances, and revocations logged in append-only tables

**Revocation API:**
- `POST /api/v1/agents/:id/revoke-cert` — Revoke agent cert (deactivates agent)
- `POST /api/v1/gateways/:id/revoke-cert` — Revoke gateway cert (deactivates gateway)
- `GET /api/v1/revoked-certificates` — List all revoked certificates

When an agent connects with a revoked certificate, the backend sends a `DisconnectAgent` message to the gateway, which immediately drops the connection.

### User Roles

AppControl defines four platform-level roles that determine baseline access:

| Role | Description |
|------|-------------|
| **admin** | Full platform access. Can manage users, teams, workspaces, agents, and all applications. Implicit `owner` on all applications. |
| **operator** | Can operate applications (start, stop, restart) where granted `operate` permission or higher. Cannot modify application configuration or manage platform settings. |
| **editor** | Can modify application configuration (add/remove components, edit commands, change dependencies) where granted `edit` permission or higher. |
| **viewer** | Read-only access. Can view application status, maps, logs, and reports where granted `view` permission or higher. Cannot perform any operations. |

Platform roles provide baseline capabilities. Application-level permissions (view, operate, edit, manage, owner) control access to specific applications.

### Authentication

AppControl supports multiple authentication mechanisms to integrate with enterprise identity providers.

#### OIDC (OpenID Connect)

Compatible with any OIDC-compliant identity provider:
- **Keycloak** (fully tested)
- **Okta** (fully tested)
- **Azure AD / Entra ID** (fully tested)
- **Any standards-compliant OIDC provider**

Configuration requires the OIDC discovery URL, client ID, and client secret. AppControl uses the authorization code flow with PKCE for browser-based authentication.

#### SAML 2.0

Compatible with SAML 2.0 identity providers:
- **ADFS** (Active Directory Federation Services)
- **Azure AD / Entra ID**
- **Okta**
- **Any standards-compliant SAML 2.0 provider**

Configuration requires the IdP metadata URL or XML, entity ID, and assertion consumer service URL. AppControl supports both SP-initiated and IdP-initiated SSO.

#### API Keys

For machine-to-machine authentication (scheduler integration, automation):
- API keys are created and managed through the Settings page
- Keys use the format `ac_<uuid>`
- Keys are passed via the `Authorization: Bearer ac_<uuid>` header
- See the [API Keys](#api-keys) section for details

#### JWT Tokens

All authenticated sessions use **RS256-signed JWT tokens** for stateless session management. Tokens contain:
- User identity and roles
- Organization membership
- Token expiration (configurable, default: 1 hour)
- Refresh token for session continuity (configurable, default: 24 hours)

### Monitoring

AppControl exposes standard endpoints for monitoring and observability:

| Endpoint | Purpose | Details |
|----------|---------|---------|
| `GET /health` | **Liveness probe** | Returns HTTP 200 if the service is running. Used by Kubernetes/OpenShift liveness probes. |
| `GET /ready` | **Readiness probe** | Returns HTTP 200 if the service is ready to accept traffic (database connected, cache available). Returns HTTP 503 if not ready. |
| `GET /metrics` | **Prometheus metrics** | Exposes metrics in Prometheus format: request latency, error rates, active WebSocket connections, agent count, operation durations, queue depths. |
| `GET /openapi.json` | **OpenAPI specification** | Full OpenAPI 3.0 specification for the REST API. Can be imported into Swagger UI, Postman, or code generators. |

**Recommended monitoring setup:**
- Configure Kubernetes/OpenShift liveness and readiness probes against `/health` and `/ready`
- Scrape `/metrics` with Prometheus at 15-second intervals
- Set up alerts for: agent disconnections, failed operations, high error rates, slow health checks
- Use the OpenAPI specification to generate client libraries for custom integrations

---

## Screenshots

Screenshots are auto-generated by CI from the running application. See the `docs/screenshots/` directory and the inline screenshots above for the latest versions.

<!-- SCREENSHOT:settings -->
*Settings — user profile and preferences*
