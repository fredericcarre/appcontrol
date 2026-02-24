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

```bash
git clone https://github.com/fredericcarre/appcontrol.git
cd appcontrol
docker compose -f docker/docker-compose.yaml up -d
```

Wait approximately 60 seconds for the images to build on first run. Verify all services are healthy:

```bash
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

Open http://localhost:8080 and click **"Dev Quick Login (admin@localhost)"** at the bottom of the login page. This logs you in instantly with the auto-seeded admin account -- no password needed in development mode.

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
docker compose -f docker/docker-compose.yaml down

# Stop and wipe all data
docker compose -f docker/docker-compose.yaml down -v
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

The Vite dev server starts on http://localhost:5173 with hot module replacement. Click **"Dev Quick Login (admin@localhost)"** on the login page to sign in instantly.

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
- **Auto-seeds a default organization and admin user** on first start (when no users exist)
- Provides a **"Dev Quick Login"** button on the login page (no password needed)
- Issues 24-hour JWT tokens stored in HttpOnly cookies

**Minimal environment:**

```bash
export JWT_SECRET=dev-secret-change-in-production
export DATABASE_URL=postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol
```

**Default dev credentials (auto-seeded on first start):**

| Field | Value |
|-------|-------|
| Email | `admin@localhost` |
| Role | `admin` |
| Organization | `Dev Org` |

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

**Manual user creation (advanced):**

If you need additional users beyond the auto-seeded admin:

```sql
-- Connect to PostgreSQL
psql postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol

-- Create an operator user
INSERT INTO users (id, organization_id, external_id, email, display_name, role)
VALUES (
  gen_random_uuid(),
  '00000000-0000-0000-0000-000000000001',
  'dev-operator',
  'operator@localhost',
  'Dev Operator',
  'operator'
);
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

---

## First Steps After Login

### 1. Create Your First Application

```bash
curl -X POST http://localhost:3000/api/v1/apps \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Payment Gateway",
    "description": "SEPA payment processing system",
    "environment": "production",
    "site_id": null
  }' | jq
```

Save the returned `id` -- you will use it in subsequent commands.

```bash
export APP_ID=<returned-uuid>
```

### 2. Add Components to the Application

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

### 3. Start the Application (DAG Sequencing)

AppControl starts components in topological order, respecting dependencies:

```
Level 0: postgres-primary    (starts first, waits for RUNNING)
Level 1: payment-api         (starts after postgres is RUNNING)
```

```bash
curl -X POST "http://localhost:3000/api/v1/apps/$APP_ID/start" \
  -H "Authorization: Bearer $TOKEN" | jq
```

### 4. Enroll an Agent

Agents run on your servers and execute health checks and commands. To enroll an agent:

**Step 1: Create an enrollment token (admin only):**

```bash
curl -X POST http://localhost:3000/api/v1/enrollment/tokens \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "datacenter-paris",
    "max_uses": 50,
    "expires_in_hours": 720
  }' | jq
```

Save the returned `token` value.

**Step 2: Install and start the agent on the target server:**

```bash
# On the target server
./appcontrol-agent \
  --gateway-url wss://gateway.corp.com:4443 \
  --name app-server-01 \
  --enrollment-token "<token-from-step-1>"
```

The agent registers with the gateway, receives its identity, and begins executing health checks for components assigned to its host.

### 5. Create an API Key for Scheduler Integration

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

## Troubleshooting

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
docker compose -f docker/docker-compose.yaml ps postgres

# Check connectivity
psql postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol -c "SELECT 1"

# If using Docker Compose, ensure the backend depends on the postgres service
# The provided docker-compose.yaml already handles this
```

### "Connection refused" on port 3000

The backend is not running or is still starting up.

**Fix:**

```bash
# Check if the backend process is running
curl -s http://localhost:3000/health || echo "Backend not reachable"

# In Docker Compose, check logs
docker compose -f docker/docker-compose.yaml logs backend

# In local dev, check terminal output for errors
```

### Database migrations fail

If migrations fail with schema conflicts, you may have a stale database.

**Fix:**

```bash
# Wipe and recreate (development only!)
docker compose -f docker/docker-compose.yaml down -v
docker compose -f docker/docker-compose.yaml up -d

# Or for local dev
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
docker compose -f docker/docker-compose.yaml logs gateway
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

## Next Steps

- **Full Configuration Reference:** See [`docs/CONFIGURATION.md`](./CONFIGURATION.md)
- **Production Deployment:** See [`docs/PRODUCTION_DEPLOYMENT.md`](./PRODUCTION_DEPLOYMENT.md)
- **Agent Installation:** See [`docs/AGENT_INSTALLATION.md`](./AGENT_INSTALLATION.md)
- **Architecture Overview:** See [`docs/architecture.md`](./architecture.md)
- **API Specification:** Browse the full OpenAPI spec at `http://localhost:3000/openapi.json`
- **CLI Reference:** Run `cargo run --bin appctl -- --help`
