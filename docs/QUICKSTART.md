# AppControl QuickStart Guide

Get AppControl v4 running in under 10 minutes. This guide covers two deployment paths: a fully containerized stack via Docker Compose, and a local development setup with hot-reload.

---

## Prerequisites

| Requirement | Docker Compose | Local Dev |
|-------------|:--------------:|:---------:|
| Docker Desktop (or Podman) | Required | Required |
| Rust 1.88+ | - | Required |
| Node.js 22+ | - | Required |
| PostgreSQL 16 | via Docker | via Docker |

---

## Option A: Docker Compose (Fastest)

This brings up the full stack in containers: PostgreSQL, backend API, frontend SPA, and gateway.

### Using pre-built images (recommended)

Pre-built images from GitHub Container Registry — no local build required:

```bash
git clone https://github.com/fredericcarre/appcontrol.git
cd appcontrol
docker compose -f docker/docker-compose.release.yaml up -d
```

Pin a specific version:

```bash
APPCONTROL_VERSION=0.2.0 docker compose -f docker/docker-compose.release.yaml up -d
```

### Building from source

If you prefer to build images locally:

```bash
git clone https://github.com/fredericcarre/appcontrol.git
cd appcontrol
docker compose -f docker/docker-compose.yaml up -d
```

Wait approximately 60 seconds for the images to build on first run.

### Verify services are running

```bash
# For pre-built images:
docker compose -f docker/docker-compose.release.yaml ps

# For local build:
docker compose -f docker/docker-compose.yaml ps
```

### Service URLs

| Service | URL | Purpose |
|---------|-----|---------|
| Frontend | http://localhost:8080 | Web UI |
| Backend API | http://localhost:3000 | REST API + WebSocket |
| Gateway | localhost:4443 | Agent WebSocket endpoint |
| PostgreSQL | localhost:5432 | Database (appcontrol/appcontrol_dev) |

### First Login

Open http://localhost:8080. The login form is pre-filled with the seeded admin email (default: `admin@localhost`). Leave the password field empty and click **Sign in**.

> **Credentials:** email = value of `SEED_ADMIN_EMAIL` (default `admin@localhost`), password = _(leave empty)_

### Seed Configuration

On first start (when the database is empty), the backend creates an initial organization and admin user. All values are configurable via environment variables in the docker-compose file:

| Variable | Default | Description |
|----------|---------|-------------|
| `SEED_ENABLED` | `true` (dev) / `false` (prod) | Whether to auto-seed an org + admin user |
| `SEED_ADMIN_EMAIL` | `admin@localhost` | Email for the seeded admin |
| `SEED_ADMIN_DISPLAY_NAME` | `Admin` | Display name for the seeded admin |
| `SEED_ORG_NAME` | `Default Organization` | Organization name |
| `SEED_ORG_SLUG` | `default` | Organization slug (URL-safe) |

To customize, edit the `SEED_*` variables in `docker/docker-compose.release.yaml` before starting.

### Verify Health

```bash
# Backend health check
curl -s http://localhost:3000/health | jq
# Expected: {"status":"ok","version":"0.1.0"}

# Backend readiness (includes database connectivity)
curl -s http://localhost:3000/ready | jq
# Expected: {"status":"ready"}

# OpenAPI specification
curl -s http://localhost:3000/openapi.json | jq '.info'
```

### Tear Down

```bash
# Stop containers (keep data)
docker compose -f docker/docker-compose.release.yaml down

# Stop and wipe all data
docker compose -f docker/docker-compose.release.yaml down -v
```

---

## Option B: Local Development (Hot-Reload)

This mode runs infrastructure in Docker but compiles and runs the Rust backend and React frontend natively for fast iteration.

### Step 1: Run the Setup Script

```bash
cd appcontrol
./docker/dev-setup.sh
```

The script:
1. Checks prerequisites (Docker, Rust, Node.js)
2. Starts PostgreSQL 16 via `docker-compose.dev.yaml`
3. Installs `sqlx-cli` if missing, then runs database migrations
4. Builds the Rust workspace (`cargo build --workspace`)
5. Installs frontend dependencies (`npm ci`)

### Step 2: Start the Four Terminals

**Terminal 1 -- Backend API**

```bash
export DATABASE_URL=postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol
export JWT_SECRET=dev-secret-change-in-production
export RUST_LOG=info,appcontrol_backend=debug
cargo run --bin appcontrol-backend
```

The backend listens on port 3000 by default. Migrations run automatically on startup.

**Terminal 2 -- Frontend (Vite hot-reload)**

```bash
cd frontend
npm run dev
```

The Vite dev server starts on http://localhost:5173 with hot module replacement. The login form is pre-filled with `admin@localhost` — leave the password empty and click **Sign in**.

**Terminal 3 -- Gateway**

```bash
export RUST_LOG=info,appcontrol_gateway=debug
cargo run --bin appcontrol-gateway
```

The gateway listens on port 4443 and bridges agent WebSocket connections to the backend. In dev mode, the gateway connects to the backend over `ws://` (plaintext). In production, always use `wss://` with TLS -- see the `BACKEND_TLS_CA_FILE` env var for internal PKI.

**Terminal 4 -- Agent (optional)**

```bash
cargo run --bin appcontrol-agent -- --gateway-url wss://localhost:4443 --name dev-agent
```

### Optional: Database Tools

```bash
docker compose -f docker/docker-compose.dev.yaml --profile tools up -d
```

| Tool | URL | Credentials |
|------|-----|-------------|
| pgAdmin | http://localhost:5050 | admin@appcontrol.local / admin |

### Tear Down

```bash
# Keep data
docker compose -f docker/docker-compose.dev.yaml down

# Wipe data
docker compose -f docker/docker-compose.dev.yaml down -v
```

---

## Authentication Setup

AppControl supports three authentication methods. They can be enabled independently or combined.

```
+----------------------------------------------------------+
|                    Authentication Flow                     |
+----------------------------------------------------------+
|                                                          |
|  Browser/CLI  --->  Backend API  --->  JWT (HttpOnly)    |
|                          |                               |
|          +---------------+------------------+            |
|          |               |                  |            |
|     JWT-only        OIDC Flow          SAML 2.0          |
|    (dev mode)    (Keycloak/Okta/    (ADFS/Azure AD/      |
|                   Azure AD)           Okta)              |
|          |               |                  |            |
|  POST /api/v1/    GET /api/v1/      GET /api/v1/        |
|  auth/login       auth/oidc/login   auth/saml/login     |
+----------------------------------------------------------+
```

### Without SAML/OIDC (Local Dev Mode)

In development mode (`APP_ENV=development`, the default), the backend:

- Accepts a simple `JWT_SECRET` for HMAC signing (no RSA key pair needed)
- Auto-runs migrations on startup, creating the database schema
- **Auto-seeds an organization and admin user** on first start (when no users exist), using `SEED_*` env vars
- Allows login with just an email (no password needed) — the login form auto-fills from `SEED_ADMIN_EMAIL`
- Issues 24-hour JWT tokens stored in HttpOnly cookies

**Minimal environment:**

```bash
export JWT_SECRET=dev-secret-change-in-production
export DATABASE_URL=postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol
```

**Default dev credentials (auto-seeded on first start, configurable via SEED_* env vars):**

| Field | Default | Env Var |
|-------|---------|---------|
| Email | `admin@localhost` | `SEED_ADMIN_EMAIL` |
| Display Name | `Admin` | `SEED_ADMIN_DISPLAY_NAME` |
| Role | `admin` (org) + `super_admin` (platform) | — |
| Organization | `Default Organization` | `SEED_ORG_NAME` |

**Obtain a JWT token:**

```bash
# Use the dev-login endpoint (development mode only)
TOKEN=$(curl -s -X POST http://localhost:3000/api/v1/auth/dev-login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost"}' | jq -r '.token')

echo $TOKEN
```

> **Note:** The `dev-login` endpoint only works when `APP_ENV=development`. It returns 404 in production.

**Use the token:**

```bash
# Via Authorization header
curl -H "Authorization: Bearer $TOKEN" http://localhost:3000/api/v1/apps | jq

# Or via API key (after creating one -- see "Create API key" below)
curl -H "X-Api-Key: ac_XXXXX" http://localhost:3000/api/v1/apps | jq
```

**User management (via API):**

Org admins can create and manage users directly through the API:

```bash
# Create an operator user
curl -X POST http://localhost:3000/api/v1/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "operator@localhost",
    "display_name": "Dev Operator",
    "role": "operator"
  }' | jq

# List all users
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/users | jq

# Update a user's role
curl -X PUT http://localhost:3000/api/v1/users/<user-uuid> \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"role": "editor"}' | jq

# Deactivate a user
curl -X PUT http://localhost:3000/api/v1/users/<user-uuid> \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"is_active": false}' | jq
```

### With OIDC (Keycloak, Okta, Azure AD)

Set these environment variables on the backend to enable OIDC:

| Variable | Example | Description |
|----------|---------|-------------|
| `OIDC_DISCOVERY_URL` | `https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration` | OIDC discovery endpoint |
| `OIDC_CLIENT_ID` | `appcontrol` | Client ID from your provider |
| `OIDC_CLIENT_SECRET` | `s3cr3t-value` | Client secret |
| `OIDC_REDIRECT_URI` | `https://appcontrol.example.com/api/v1/auth/oidc/callback` | Callback URL |
| `OIDC_SCOPES` | `openid,profile,email` | Scopes (default: `openid,profile,email`) |

**Keycloak example with docker-compose override:**

Create `docker/docker-compose.oidc.yaml`:

```yaml
version: "3.8"

services:
  keycloak:
    image: quay.io/keycloak/keycloak:24.0
    command: start-dev --import-realm
    environment:
      KEYCLOAK_ADMIN: admin
      KEYCLOAK_ADMIN_PASSWORD: admin
    ports:
      - "8180:8080"

  backend:
    environment:
      OIDC_DISCOVERY_URL: http://keycloak:8080/realms/appcontrol/.well-known/openid-configuration
      OIDC_CLIENT_ID: appcontrol
      OIDC_CLIENT_SECRET: change-me
      OIDC_REDIRECT_URI: http://localhost:3000/api/v1/auth/oidc/callback
```

Start the stack:

```bash
docker compose \
  -f docker/docker-compose.yaml \
  -f docker/docker-compose.oidc.yaml \
  up -d
```

After Keycloak starts, create the `appcontrol` realm and client at http://localhost:8180/admin (admin/admin), then:

1. Create realm `appcontrol`
2. Create client `appcontrol` with "Client authentication" enabled
3. Set valid redirect URI: `http://localhost:3000/api/v1/auth/oidc/callback`
4. Copy the client secret to `OIDC_CLIENT_SECRET`

**Login flow:**

```
Browser --> GET /api/v1/auth/oidc/login --> 302 to Keycloak
Keycloak authenticates user --> 302 to /api/v1/auth/oidc/callback?code=XXX
Backend exchanges code --> creates/updates user --> returns JWT (HttpOnly cookie)
```

### With SAML 2.0 (ADFS, Azure AD, Okta)

Set these environment variables on the backend to enable SAML:

| Variable | Example | Description |
|----------|---------|-------------|
| `SAML_IDP_SSO_URL` | `https://adfs.corp.com/adfs/ls/` | IdP SSO endpoint |
| `SAML_IDP_CERT` | `MIICpDCCAYw...` | IdP signing certificate (PEM, base64) |
| `SAML_SP_ENTITY_ID` | `https://appcontrol.example.com/saml` | SP entity ID |
| `SAML_SP_ACS_URL` | `https://appcontrol.example.com/api/v1/auth/saml/acs` | Assertion Consumer Service URL |
| `SAML_GROUP_ATTRIBUTE` | `memberOf` | Attribute name for group claims (default: `memberOf`) |
| `SAML_EMAIL_ATTRIBUTE` | `email` | Attribute name for email (default: `email`) |
| `SAML_NAME_ATTRIBUTE` | `displayName` | Attribute name for display name (default: `displayName`) |
| `SAML_ADMIN_GROUP` | `CN=APPCONTROL_ADMINS,OU=Groups,DC=corp,DC=com` | Group mapped to org admin role |
| `SAML_WANT_ASSERTIONS_SIGNED` | `true` | Require signed assertions (default: `true`) |

**ADFS example:**

```bash
export SAML_IDP_SSO_URL=https://adfs.corp.com/adfs/ls/
export SAML_IDP_CERT="$(cat /path/to/adfs-signing-cert.pem | base64 -w0)"
export SAML_SP_ENTITY_ID=https://appcontrol.corp.com/saml
export SAML_SP_ACS_URL=https://appcontrol.corp.com/api/v1/auth/saml/acs
export SAML_ADMIN_GROUP="CN=APPCONTROL_ADMINS,OU=Groups,DC=corp,DC=com"
```

**Configure your IdP with AppControl's SP metadata:**

```bash
curl http://localhost:3000/api/v1/auth/saml/metadata
```

This returns the SP metadata XML that you import into your IdP (ADFS Relying Party Trust, Azure AD Enterprise Application, etc.).

**SAML group-to-team mapping:**

SAML groups are automatically synchronized to AppControl teams on each login. Configure mappings via the admin API:

```bash
# Map an AD group to an AppControl team with "operate" permission
curl -X POST http://localhost:3000/api/v1/saml/group-mappings \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "saml_group": "CN=APP_PAYMENTS_OPERATORS,OU=Groups,DC=corp,DC=com",
    "team_id": "<team-uuid>",
    "default_role": "operator"
  }'

# List current group mappings
curl http://localhost:3000/api/v1/saml/group-mappings \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Login flow:**

```
Browser --> GET /api/v1/auth/saml/login --> 302 to IdP SSO URL (with AuthnRequest)
IdP authenticates user --> POST /api/v1/auth/saml/acs (with SAMLResponse)
Backend validates assertion --> syncs groups to teams --> sets HttpOnly cookie --> redirects to app
```

### User Provisioning (Production)

In production, users are **not created manually** — they are provisioned automatically by your identity provider (OIDC or SAML) on first login:

```
User authenticates via OIDC/SAML
    → Backend receives identity claims (email, name, groups)
    → User record is created/updated in AppControl
    → SAML groups are mapped to AppControl teams (if configured)
    → User receives JWT token and is logged in
```

**How to add users in production:**

1. Add users to the appropriate groups in your IdP (Active Directory, Okta, Azure AD, etc.)
2. Configure SAML group mappings so IdP groups map to AppControl teams:
   ```bash
   curl -X POST http://localhost:3000/api/v1/saml/group-mappings \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "saml_group": "CN=APP_OPERATORS,OU=Groups,DC=corp,DC=com",
       "team_id": "<team-uuid>",
       "default_role": "operator"
     }'
   ```
3. Users log in via SSO — their account and team membership is created automatically
4. The admin maps `SAML_ADMIN_GROUP` to grant org admin role to specific IdP groups

**Roles assigned at login:**
- Users in `SAML_ADMIN_GROUP` → `admin` role (platform super-admin)
- Users with SAML group mappings → role from mapping (`operator`, `editor`, `viewer`)
- Users without group mapping → `viewer` role (default, read-only)

---

## Understanding the Admin Hierarchy

AppControl distinguishes two levels of administration:

### Platform Super-Admin (`platform_role: super_admin`)

The super-admin is a **platform-level** role. Super-admins can:
- **Create and manage organizations** (`POST /api/v1/organizations`)
- View all organizations on the platform
- The dev seed user (`admin@localhost`) is automatically a super-admin

### Org Admin (`role: admin`)

The org admin manages **a single organization**. Org admins can:
- Create and manage **sites** (`POST /api/v1/sites`)
- Create and manage **users** (`POST /api/v1/users`)
- Create **enrollment tokens** for gateways and agents
- Manage **teams**, **permissions**, and **workspaces**
- Perform all operations on all applications (implicit `owner`)
- Access **break-glass** emergency controls
- Revoke agent/gateway certificates

### Dev Mode Default User

When you start AppControl in development mode, **a platform admin is automatically created**:

| Field | Value |
|-------|-------|
| Email | `admin@localhost` |
| Org Role | `admin` (org administrator) |
| Platform Role | `super_admin` (can create orgs) |
| Organization | `Dev Org` |

In production, the super-admin is typically the first user who logs in via OIDC/SAML with the `SAML_ADMIN_GROUP` group. Super-admin status must then be granted explicitly in the database:

```sql
UPDATE users SET platform_role = 'super_admin' WHERE email = 'first-admin@corp.com';
```

### Other Roles

| Role | Can Do |
|------|--------|
| `operator` | Start, stop, restart applications (where granted `operate` permission) |
| `editor` | Modify application config: components, dependencies, commands (where granted `edit` permission) |
| `viewer` | Read-only: view status, maps, logs, reports (where granted `view` permission) |

---

## First Steps After Login

The typical setup flow follows this sequence:

```
1. Login as admin
2. Create sites (datacenters/environments)
3. Create gateway enrollment tokens (scoped to "gateway")
4. Deploy and enroll gateways on each site
5. Create agent enrollment tokens (scoped to "agent")
6. Deploy and enroll agents on target servers
7. Create applications with components
8. Start operating
```

### 1. Create Sites

Sites represent your datacenters, DR sites, or environments. Applications, gateways, and agents are organized by site.

```bash
# Create a primary site
curl -X POST http://localhost:3000/api/v1/sites \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Paris Datacenter",
    "code": "PAR1",
    "site_type": "primary",
    "location": "Paris, France"
  }' | jq
```

```bash
export SITE_ID=<returned-id>
```

For DR setups, create a secondary site:

```bash
curl -X POST http://localhost:3000/api/v1/sites \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "London DR Site",
    "code": "LON1",
    "site_type": "dr",
    "location": "London, UK"
  }' | jq
```

List all sites:

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/sites | jq
```

### 2. Enroll a Gateway

Gateways bridge agent WebSocket connections to the backend. Each site typically has one or more gateways.

**Step 1: Create a gateway enrollment token (admin only):**

```bash
curl -X POST http://localhost:3000/api/v1/enrollment/tokens \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "gateway-paris-dc",
    "scope": "gateway",
    "max_uses": 5,
    "valid_hours": 720
  }' | jq
```

> **Important:** The returned `token` (starting with `ac_enroll_`) is shown **only once**. Copy it immediately.

**Step 2: Deploy and start the gateway on the site:**

```bash
# On the gateway server in Paris DC
./appcontrol-gateway \
  --backend-url wss://backend.corp.com:3000/ws/gateway \
  --enrollment-token "<token-from-step-1>" \
  --listen-addr 0.0.0.0:4443
```

The gateway enrolls with the backend, receives its mTLS certificate signed by the organization CA, and begins accepting agent connections.

### 3. Enroll Agents

Agents run on your servers and execute health checks and commands.

**Step 1: Create an agent enrollment token (admin only):**

```bash
curl -X POST http://localhost:3000/api/v1/enrollment/tokens \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "agents-paris-dc",
    "scope": "agent",
    "max_uses": 50,
    "valid_hours": 720
  }' | jq
```

Save the returned `token` value.

**Step 2: Install and start the agent on each target server:**

```bash
# On the target server
./appcontrol-agent \
  --gateway-url wss://gateway-paris.corp.com:4443 \
  --name app-server-01 \
  --enrollment-token "<token-from-step-1>"
```

The agent registers with the gateway, receives its mTLS identity, and begins executing health checks for components assigned to its host.

> **Tip:** You can use the same agent enrollment token for multiple servers (up to `max_uses`). Create separate tokens per site or team for better audit trail.

**Or use the CLI:**

```bash
appctl pki create-token --name "agents-london-dr" --scope agent --max-uses 20 --valid-hours 168
```

### 4. Create Your First Application

```bash
curl -X POST http://localhost:3000/api/v1/apps \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Payment Gateway",
    "description": "SEPA payment processing system",
    "site_id": "'$SITE_ID'"
  }' | jq
```

Save the returned `id`:

```bash
export APP_ID=<returned-uuid>
```

### 5. Add Components to the Application

Components represent the individual services, databases, and processes that make up your application.

```bash
# Add a database component
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/components" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "postgres-primary",
    "display_name": "PostgreSQL Primary",
    "host": "db-server-01.corp.com",
    "check_cmd": "pg_isready -h localhost -p 5432",
    "start_cmd": "systemctl start postgresql",
    "stop_cmd": "systemctl stop postgresql",
    "check_interval_seconds": 30
  }' | jq

# Add an application server
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/components" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "payment-api",
    "display_name": "Payment API Server",
    "host": "app-server-01.corp.com",
    "check_cmd": "curl -sf http://localhost:8080/health",
    "start_cmd": "systemctl start payment-api",
    "stop_cmd": "systemctl stop payment-api",
    "check_interval_seconds": 30
  }' | jq
```

Add a dependency so the API server starts after the database:

```bash
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/dependencies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "from_component_id": "<payment-api-uuid>",
    "to_component_id": "<postgres-primary-uuid>"
  }' | jq
```

### 6. Start the Application (DAG Sequencing)

AppControl starts components in topological order, respecting dependencies:

```
Level 0: postgres-primary    (starts first, waits for RUNNING)
Level 1: payment-api         (starts after postgres is RUNNING)
```

```bash
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/start" \
  -H "Authorization: Bearer $TOKEN" | jq
```

### 7. Create an API Key for Scheduler Integration

API keys enable schedulers (Control-M, AutoSys, Dollar Universe, TWS) to trigger operations without interactive login:

```bash
curl -X POST http://localhost:3000/api/v1/api-keys \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "controlm-production",
    "allowed_actions": ["start", "stop", "status"]
  }' | jq
```

**Important:** The full key (starting with `ac_`) is returned only once. Store it securely.

Use the API key with the orchestration endpoints:

```bash
curl -X POST "http://localhost:3000/api/v1/orchestration/apps/$APP_ID/start" \
  -H "X-Api-Key: ac_XXXXX"
```

---

## Quick API Reference

All endpoints are prefixed with `/api/v1/`. Authentication is via `Authorization: Bearer <jwt>` header, HttpOnly cookie, or `X-Api-Key: ac_XXXXX` header.

### Applications

```bash
# List all applications
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/apps | jq

# Create application
curl -X POST http://localhost:3000/api/v1/apps \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "My App", "description": "Description"}' | jq

# Get application details
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID" | jq

# Start application (DAG-ordered)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/start" | jq

# Stop application (reverse DAG order)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/stop" | jq

# Restart failed branch only
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/start-branch" | jq
```

### Orchestration (Scheduler Integration)

These endpoints are designed for external schedulers:

```bash
# Start app (returns immediately with operation ID)
curl -X POST "http://localhost:3000/api/v1/orchestration/apps/$APP_ID/start" \
  -H "X-Api-Key: ac_XXXXX" | jq

# Stop app
curl -X POST "http://localhost:3000/api/v1/orchestration/apps/$APP_ID/stop" \
  -H "X-Api-Key: ac_XXXXX" | jq

# Check current status
curl "http://localhost:3000/api/v1/orchestration/apps/$APP_ID/status" \
  -H "X-Api-Key: ac_XXXXX" | jq

# Wait for RUNNING state (long-poll, blocks until ready or timeout)
curl "http://localhost:3000/api/v1/orchestration/apps/$APP_ID/wait-running?timeout=300" \
  -H "X-Api-Key: ac_XXXXX" | jq
```

### Components

```bash
# List components for an app
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/components" | jq

# Start a single component
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/components/$COMPONENT_ID/start" | jq

# Execute a custom command on a component
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/components/$COMPONENT_ID/command/flush-cache" | jq
```

### Diagnostics & DR

```bash
# Run 3-level diagnostic on an application
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/diagnose" | jq

# Initiate DR switchover
curl -X POST -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/switchover" | jq
```

### Organizations (Super-Admin Only)

```bash
# List all organizations (super-admin only)
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/organizations | jq

# Create a new organization with its initial admin
curl -X POST http://localhost:3000/api/v1/organizations \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Acme Corp",
    "slug": "acme-corp",
    "admin_email": "admin@acme.com",
    "admin_display_name": "Acme Admin"
  }' | jq
# → Returns org details + admin_user_id. PKI (CA) is auto-initialized.
```

### Sites (Admin Only)

```bash
# List sites
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/sites | jq

# Create site
curl -X POST http://localhost:3000/api/v1/sites \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Paris DC", "code": "PAR1", "site_type": "primary", "location": "Paris"}' | jq

# Update site
curl -X PUT "http://localhost:3000/api/v1/sites/$SITE_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"location": "Paris, France"}' | jq

# Delete site (fails if applications are linked)
curl -X DELETE "http://localhost:3000/api/v1/sites/$SITE_ID" \
  -H "Authorization: Bearer $TOKEN" | jq
```

### Users (Admin Only)

```bash
# List users (with filters)
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/users?role=operator&is_active=true" | jq

# Create local user
curl -X POST http://localhost:3000/api/v1/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"email": "operator@corp.com", "display_name": "Jane Doe", "role": "operator"}' | jq

# Update user role
curl -X PUT "http://localhost:3000/api/v1/users/$USER_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"role": "editor"}' | jq

# Deactivate user
curl -X PUT "http://localhost:3000/api/v1/users/$USER_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"is_active": false}' | jq

# Get current user info
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/users/me | jq
```

### Gateways (Admin Only)

```bash
# List gateways
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/gateways | jq

# Assign gateway to a site
curl -X PUT "http://localhost:3000/api/v1/gateways/$GW_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"site_id": "'$SITE_ID'"}' | jq
```

### Certificate Revocation (Admin Only)

```bash
# Revoke an agent's certificate (deactivates the agent)
curl -X POST "http://localhost:3000/api/v1/agents/$AGENT_ID/revoke-cert" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason": "Agent compromised — server decommissioned"}' | jq

# Revoke a gateway's certificate (deactivates the gateway)
curl -X POST "http://localhost:3000/api/v1/gateways/$GW_ID/revoke-cert" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason": "Gateway certificate leaked"}' | jq

# List all revoked certificates
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/revoked-certificates | jq
```

### Health & Observability

```bash
# Health check (no auth required)
curl http://localhost:3000/health | jq

# Readiness probe (no auth required, checks DB)
curl http://localhost:3000/ready | jq

# Prometheus metrics (no auth required)
curl http://localhost:3000/metrics

# OpenAPI 3.0 specification (no auth required)
curl http://localhost:3000/openapi.json | jq
```

### API Keys

```bash
# Create API key
curl -X POST http://localhost:3000/api/v1/api-keys \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "scheduler-key", "allowed_actions": ["start", "stop", "status"]}' | jq

# List API keys
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/api-keys | jq

# Revoke API key
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/api-keys/$KEY_ID"
```

---

## Security: Preventing Gateway & Agent Impersonation

AppControl uses multiple layers to ensure that gateways and agents cannot be spoofed:

### Layer 1: Token-Based Enrollment

- Enrollment tokens are **SHA-256 hashed** — the plaintext is never stored in the database
- Tokens have **expiration**, **max usage counts**, and can be **revoked** at any time
- Each enrollment attempt (success or failure) is logged in the **append-only** `enrollment_events` table
- Tokens are **scoped** (`gateway` or `agent`) — a gateway token cannot enroll an agent and vice versa

### Layer 2: mTLS (Mutual TLS)

- Each organization has its own **Certificate Authority (CA)**, auto-generated at startup
- During enrollment, the backend signs a certificate for the agent/gateway using the org's CA private key
- After enrollment, **all connections are mTLS**: both sides present certificates signed by the same CA
- Gateway certificates include `ServerAuth + ClientAuth` extended key usage and support SANs (DNS/IP)
- Agent certificates include the hostname as CN and SAN

### Layer 3: Certificate Pinning

- The **SHA-256 fingerprint** of each issued certificate is stored in the database (`agents.certificate_fingerprint`, `gateways.certificate_fingerprint`)
- On every connection, the gateway extracts the client certificate fingerprint and verifies it matches the stored value
- This prevents a valid cert from a **different** agent being reused on an unauthorized host

### Layer 4: Certificate Revocation

- Admins can **immediately revoke** any agent or gateway certificate via the API
- Revoked fingerprints are stored in the `revoked_certificates` table (append-only)
- The gateway checks this table on each connection — a revoked cert is rejected even if it's still cryptographically valid
- Revocation also deactivates the agent/gateway record and marks `identity_verified = false`

### Layer 5: Audit Trail

Every security event is logged with full traceability:
- `enrollment_events` — who enrolled, when, from what IP, with what token
- `certificate_events` — cert issued, renewed, revoked, with fingerprints
- `action_log` — admin actions (token creation, cert revocation, etc.)

---

## Troubleshooting

### Can't log in / login page shows an error

In development mode, login with the email from `SEED_ADMIN_EMAIL` (default: `admin@localhost`) and leave the password empty.

**Fix:**

1. Make sure the backend is healthy: `curl http://localhost:3000/health`
2. Check backend logs: `docker compose -f docker/docker-compose.release.yaml logs backend`
3. Check the login form is pre-filled: `curl http://localhost:3000/api/v1/auth/info` — should return `{"dev_mode":true,"default_email":"admin@localhost"}`
4. The admin account is auto-seeded on first startup when no users exist. If the database already has users from a previous run, the seed is skipped. To reset: `docker compose -f docker/docker-compose.release.yaml down -v && docker compose -f docker/docker-compose.release.yaml up -d`

### Backend fails to start: "FATAL: JWT_SECRET must be set"

This occurs when `APP_ENV=production` and `JWT_SECRET` is not set or is insecure.

**Fix:** Set a strong random secret (>= 32 characters):

```bash
export JWT_SECRET=$(openssl rand -base64 48)
```

In development mode (`APP_ENV=development`, the default), the backend uses a fallback secret automatically.

### Backend fails to start: "FATAL: DATABASE_URL must be set"

PostgreSQL is not reachable or `DATABASE_URL` is not set.

**Fix:**

```bash
# Verify PostgreSQL is running
docker compose -f docker/docker-compose.release.yaml ps postgres

# Check connectivity
psql postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol -c "SELECT 1"

# If using Docker Compose, ensure the backend depends on the postgres service
# The provided compose files already handle this
```

### "Connection refused" on port 3000

The backend is not running or is still starting up.

**Fix:**

```bash
# Check if the backend process is running
curl -s http://localhost:3000/health || echo "Backend not reachable"

# In Docker Compose, check logs
docker compose -f docker/docker-compose.release.yaml logs backend

# In local dev, check terminal output for errors
```

### Database migrations fail

If migrations fail with schema conflicts, you may have a stale database.

**Fix:**

```bash
# Wipe and recreate (development only!)
docker compose -f docker/docker-compose.release.yaml down -v
docker compose -f docker/docker-compose.release.yaml up -d

# Or for local dev (source build)
docker compose -f docker/docker-compose.dev.yaml down -v
docker compose -f docker/docker-compose.dev.yaml up -d
```

### Agent cannot connect to gateway

**Fix:**

```bash
# Verify gateway is running
curl -v wss://localhost:4443 2>&1 | head -5

# Check agent logs for TLS errors
export RUST_LOG=debug
./appcontrol-agent --gateway-url wss://localhost:4443 --name test

# In Docker, check gateway logs
docker compose -f docker/docker-compose.release.yaml logs gateway
```

### CORS errors in the browser

The frontend cannot reach the backend due to CORS policy.

**Fix:**

```bash
# In development, CORS is permissive by default (no config needed)

# In production, set allowed origins explicitly
export CORS_ORIGINS=https://appcontrol.example.com

# If frontend and backend are on different ports in dev
export CORS_ORIGINS=http://localhost:5173,http://localhost:8080
```

### SAML login redirects but never comes back

The Assertion Consumer Service URL configured in your IdP does not match `SAML_SP_ACS_URL`.

**Fix:**

1. Verify the ACS URL in the IdP matches exactly: `https://appcontrol.example.com/api/v1/auth/saml/acs`
2. Export SP metadata and re-import into your IdP: `curl http://localhost:3000/api/v1/auth/saml/metadata`
3. Check backend logs for SAML validation errors

### OIDC callback returns 502

The backend cannot reach the OIDC provider's token endpoint.

**Fix:**

```bash
# Verify the discovery URL is reachable from the backend
curl -s $OIDC_DISCOVERY_URL | jq .token_endpoint

# In Docker, ensure the backend container can resolve the OIDC provider hostname
# Add the provider to docker-compose networking or use a publicly reachable URL
```

### "Permission denied" (403) on API calls

Your user does not have sufficient permissions on the target application.

**Fix:**

Permission levels: `view` < `operate` < `edit` < `manage` < `owner`.

- `view`: Read-only access
- `operate`: Start, stop, execute commands
- `edit`: Modify components and configuration
- `manage`: Grant permissions to others
- `owner`: Full control including delete

```bash
# Check your effective permission
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/v1/apps/$APP_ID/permissions/effective" | jq

# Grant operate permission to a user (requires manage or owner)
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/permissions/users" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "<user-uuid>", "level": "operate"}' | jq
```

Org admins (`role: "admin"`) have implicit owner access on all applications.

---

## Passive Discovery Mode

Instead of manually defining every component and dependency, you can let AppControl discover your application topology automatically. Agents scan running processes, listening ports, and network connections on their hosts, then the backend infers a candidate dependency graph.

### Trigger a Discovery Scan

```bash
# Trigger discovery on a specific agent
curl -X POST "http://localhost:3000/api/v1/discovery/trigger/$AGENT_ID" \
  -H "Authorization: Bearer $TOKEN" | jq

# List discovery reports received from agents
curl "http://localhost:3000/api/v1/discovery/reports" \
  -H "Authorization: Bearer $TOKEN" | jq

# Run inference to create a draft application from agent reports
curl -X POST "http://localhost:3000/api/v1/discovery/infer" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-discovered-app", "agent_ids": ["AGENT_UUID_1", "AGENT_UUID_2"]}' | jq

# List inferred drafts
curl "http://localhost:3000/api/v1/discovery/drafts" \
  -H "Authorization: Bearer $TOKEN" | jq

# Get draft details (components + dependencies)
curl "http://localhost:3000/api/v1/discovery/drafts/$DRAFT_ID" \
  -H "Authorization: Bearer $TOKEN" | jq

# Apply a draft to create a real application
curl -X POST "http://localhost:3000/api/v1/discovery/drafts/$DRAFT_ID/apply" \
  -H "Authorization: Bearer $TOKEN" | jq
```

The draft is not applied automatically. Review the inferred topology, then call the apply endpoint to create a real application with components and dependencies.

### Operation Time Estimates

AppControl tracks historical execution times and provides P50/P95 estimates for operations:

```bash
# Get estimated start/stop time for an application
curl "http://localhost:3000/api/v1/apps/$APP_ID/estimates?operation=start" \
  -H "Authorization: Bearer $TOKEN" | jq
```

Returns per-component timing (P50/P95), DAG-aware wall-clock estimates (accounting for parallel levels), and confidence levels based on historical sample count.

### Air-Gap Agent Update

For environments without internet access, update agents via the backend WebSocket:

```bash
# Upload an agent binary
curl -X POST "http://localhost:3000/api/v1/admin/agent-binaries" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"version": "1.2.0", "platform": "linux-amd64", "binary_base64": "...", "checksum_sha256": "..."}' | jq

# Push update to a specific agent (sends binary in 256KB chunks via WebSocket)
curl -X POST "http://localhost:3000/api/v1/admin/agents/$AGENT_ID/update" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"version": "1.2.0"}' | jq

# Monitor update progress
curl "http://localhost:3000/api/v1/admin/agent-update-tasks" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## MCP Server (AI Integration)

AppControl ships with a standalone MCP (Model Context Protocol) server that exposes 10 tools for AI assistants like Claude Desktop. It communicates over JSON-RPC via stdio, so no network ports are opened.

### Setup with Claude Desktop

1. Build the MCP server binary:

```bash
cargo build --release --bin appcontrol-mcp
```

2. Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "appcontrol": {
      "command": "/path/to/appcontrol-mcp",
      "args": ["--api-url", "http://localhost:3000", "--api-key", "ac_XXXXX"]
    }
  }
}
```

3. Restart Claude Desktop. You can now ask Claude to list applications, check status, start/stop apps, view topology, run diagnostics, and more -- all through natural language.

**Available tools:** `list_apps`, `get_app_status`, `start_app`, `stop_app`, `diagnose_app`, `get_incidents`, `get_topology`, `estimate_time`, `get_activity`, `list_agents`.

---

## Next Steps

- **Scheduler Integration:** See [`docs/INTEGRATION_COOKBOOK.md`](./INTEGRATION_COOKBOOK.md) for Control-M, AutoSys, Dollar Universe, Jenkins, GitLab CI examples
- **Monitoring:** Run `docker compose -f docker/docker-compose.yaml -f docker/docker-compose.monitoring.yaml up -d` for Prometheus + Grafana (http://localhost:3001, admin/admin)
- **Full Configuration Reference:** See [`docs/CONFIGURATION.md`](./CONFIGURATION.md)
- **Production Deployment:** See [`docs/PRODUCTION_DEPLOYMENT.md`](./PRODUCTION_DEPLOYMENT.md)
- **Agent Installation:** See [`docs/AGENT_INSTALLATION.md`](./AGENT_INSTALLATION.md)
- **Architecture Overview:** See [`docs/architecture.md`](./architecture.md)
- **API Specification:** Browse the full OpenAPI spec at `http://localhost:3000/openapi.json`
- **CLI Reference:** Run `cargo run --bin appctl -- --help`
