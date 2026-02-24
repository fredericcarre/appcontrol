# Configuration Reference

Complete configuration reference for all AppControl components.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Users & Authentication](#users--authentication)
- [Backend (API Server)](#backend-api-server)
- [Gateway](#gateway)
- [Agent](#agent)
- [Frontend](#frontend)
- [CLI (appctl)](#cli-appctl)
- [TLS / mTLS Certificates](#tls--mtls-certificate-configuration)
- [Docker Compose Reference](#docker-compose-reference)
- [Production Checklist](#production-checklist)

---

## Architecture Overview

```
┌─────────────┐        ┌──────────────────┐        ┌──────────────────┐
│  Frontend    │──HTTP──│  Backend (API)   │──SQL───│  PostgreSQL 16   │
│  React SPA   │  :8080 │  Rust + Axum     │  :5432 │                  │
│  (nginx)     │        │  :3000           │        │                  │
└─────────────┘        └────────┬─────────┘        └──────────────────┘
                                │ WebSocket
                                │ /ws/gateway
                       ┌────────┴─────────┐
                       │  Gateway          │
                       │  Rust + Axum      │
                       │  :4443            │
                       └────────┬─────────┘
                                │ WebSocket (mTLS)
                    ┌───────────┼───────────┐
               ┌────┴────┐ ┌───┴─────┐ ┌───┴─────┐
               │ Agent 1 │ │ Agent 2 │ │ Agent N │
               │ (host)  │ │ (host)  │ │ (host)  │
               └─────────┘ └─────────┘ └─────────┘
```

**Key connectivity rules:**
- Frontend → Backend: HTTP/HTTPS (reverse-proxied by nginx)
- Backend → PostgreSQL: TCP/TLS (SQL)
- Backend ← Gateway: WebSocket (the gateway initiates the connection)
- Gateway ← Agents: WebSocket with mTLS (agents initiate the connection)
- **Agents never connect directly to the backend or database**
- **Gateways never connect to the database**

---

## Users & Authentication

AppControl supports three authentication methods. Only one needs to be active at a time, although OIDC and SAML can coexist.

### Authentication Methods Summary

| Method | Use Case | Configuration |
|--------|----------|---------------|
| **Dev Login** | Local development, quickstart | Automatic when `APP_ENV=development` |
| **OIDC** | Enterprise SSO (Keycloak, Okta, Azure AD, Google) | Set `OIDC_DISCOVERY_URL` |
| **SAML 2.0** | Enterprise SSO (ADFS, Azure AD, Okta, Shibboleth) | Set `SAML_IDP_SSO_URL` |

### Development Mode — Default User

When `APP_ENV=development` (the default), the backend automatically seeds a default admin user on first startup if the database is empty.

| Field | Value |
|-------|-------|
| **Email** | `admin@localhost` |
| **Display name** | `Dev Admin` |
| **Role** | `admin` (full access) |
| **Organization** | `Dev Org` |
| **Password** | _(none — dev mode does not require a password)_ |

**To log in via the UI:** Open http://localhost:8080, enter `admin@localhost` in the email field, type any value in the password field (it is ignored in dev mode), and click **Sign in**.

**To log in via the API:**

```bash
TOKEN=$(curl -s -X POST http://localhost:3000/api/v1/auth/dev-login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost"}' | jq -r '.token')
```

> **Warning:** The `dev-login` and `login` endpoints are only available when `APP_ENV=development`. In production, they return `404 Not Found` — you must configure OIDC or SAML.

### User Roles

| Role | Scope | Description |
|------|-------|-------------|
| `admin` | Organization-wide | Implicit owner on all applications. Can manage users, teams, and organization settings. |
| `viewer` | Default for new SSO users | Read-only access. Actual permissions depend on team memberships and per-app grants. |

### Permission Levels (per application)

Permissions are evaluated per application. The effective permission is the **maximum** of all grants:

```
view < operate < edit < manage < owner
```

| Level | Can do |
|-------|--------|
| `view` | See the application map, status, and logs |
| `operate` | Start, stop, restart, run diagnostics |
| `edit` | Modify components, commands, dependencies |
| `manage` | Manage permissions, share links |
| `owner` | Delete the application, transfer ownership |

**Effective permission = MAX(direct_user_grant, all_team_grants)**. Organization admins have implicit `owner` on everything.

### Teams

Teams group users and grant them permissions on applications. Teams can be managed via the API or auto-synced from SAML groups.

```bash
# Create a team
curl -X POST http://localhost:3000/api/v1/teams \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Payments-Ops", "description": "Payment system operators"}'

# Grant team permission on an application
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/permissions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"team_id": "<team-uuid>", "level": "operate"}'
```

### API Keys

API keys provide non-interactive authentication for CLI tools and scheduler integrations.

| Property | Details |
|----------|---------|
| **Format** | `ac_` prefix + random string (e.g., `ac_xK9m2pQ...`) |
| **Storage** | SHA-256 hash stored in `api_keys` table (the raw key is never stored) |
| **Scope** | Same permissions as the user who created them |
| **Expiry** | Optional — configurable at creation time |

```bash
# Create an API key (via API)
curl -X POST http://localhost:3000/api/v1/api-keys \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "scheduler-prod", "expires_in_days": 365}'

# Use with CLI
export APPCONTROL_API_KEY=ac_xK9m2pQ...
appctl status $APP_ID

# Use with curl
curl -H "Authorization: Bearer ac_xK9m2pQ..." http://localhost:3000/api/v1/apps
```

### JWT Token Details

| Property | Value |
|----------|-------|
| **Algorithm** | HS256 (HMAC-SHA256) |
| **Expiry** | 24 hours |
| **Storage** | HttpOnly cookie (browser), Bearer header (API/CLI) |
| **Claims** | `sub` (user_id), `org` (organization_id), `email`, `role`, `exp`, `iat`, `iss` |
| **Issuer** | Configurable via `JWT_ISSUER` (default: `appcontrol`) |

### OIDC Configuration

OIDC implements the Authorization Code Flow. Tested with Keycloak, Okta, Azure AD, and Google Workspace.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OIDC_DISCOVERY_URL` | **Yes** (to enable) | - | OpenID Connect discovery URL. Example: `https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration` |
| `OIDC_CLIENT_ID` | **Yes** | - | Client ID registered with the OIDC provider |
| `OIDC_CLIENT_SECRET` | **Yes** | - | Client secret |
| `OIDC_REDIRECT_URI` | No | `/api/v1/auth/oidc/callback` | Redirect URI after authentication. Must match the provider configuration exactly. |
| `OIDC_SCOPES` | No | `openid,profile,email` | Comma-separated OIDC scopes to request |

**OIDC Flow:**

```
User clicks "Sign in with SSO"
  → Browser redirects to /api/v1/auth/oidc/login
  → Backend redirects to OIDC provider (authorization_endpoint)
  → User authenticates at the OIDC provider
  → Provider redirects back to /api/v1/auth/oidc/callback?code=...
  → Backend exchanges code for tokens (token_endpoint)
  → Backend fetches user info (userinfo_endpoint)
  → Backend creates/updates user in PostgreSQL
  → Backend returns JWT (HttpOnly cookie + JSON response)
  → User is authenticated
```

**OIDC provider setup (Keycloak example):**

1. Create a realm `appcontrol`
2. Create a client `appcontrol-frontend` with **confidential** access type
3. Set valid redirect URIs: `https://appcontrol.example.com/api/v1/auth/oidc/callback`
4. Note the client ID and client secret
5. Set environment variables on the backend:

```bash
OIDC_DISCOVERY_URL=https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration
OIDC_CLIENT_ID=appcontrol-frontend
OIDC_CLIENT_SECRET=your-client-secret
OIDC_REDIRECT_URI=https://appcontrol.example.com/api/v1/auth/oidc/callback
```

**Auto-provisioning:** Users authenticating via OIDC for the first time are automatically created with the `viewer` role in the default organization.

### SAML 2.0 Configuration

SAML implements the SP-Initiated Web Browser SSO Profile. Tested with ADFS, Azure AD, and Okta.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SAML_IDP_SSO_URL` | **Yes** (to enable) | - | Identity Provider SSO endpoint URL |
| `SAML_IDP_CERT` | **Yes** | - | IdP signing certificate (PEM format, base64-encoded) |
| `SAML_SP_ENTITY_ID` | **Yes** | - | Service Provider entity ID (e.g., `https://appcontrol.example.com/saml`) |
| `SAML_SP_ACS_URL` | **Yes** | - | Assertion Consumer Service URL (e.g., `https://appcontrol.example.com/api/v1/auth/saml/acs`) |
| `SAML_GROUP_ATTRIBUTE` | No | `memberOf` | SAML attribute name containing group memberships |
| `SAML_EMAIL_ATTRIBUTE` | No | `email` | SAML attribute name for user email |
| `SAML_NAME_ATTRIBUTE` | No | `displayName` | SAML attribute name for display name |
| `SAML_ADMIN_GROUP` | No | - | SAML group name that maps to the `admin` role |
| `SAML_WANT_ASSERTIONS_SIGNED` | No | `true` | Require IdP to sign SAML assertions |

**SAML Flow:**

```
User clicks "Sign in with SSO" (configured for SAML)
  → Browser redirects to /api/v1/auth/saml/login
  → Backend generates AuthnRequest and redirects to IdP (SAML_IDP_SSO_URL)
  → User authenticates at the IdP
  → IdP POSTs SAMLResponse to /api/v1/auth/saml/acs
  → Backend validates response, extracts attributes
  → Backend syncs SAML groups → AppControl teams
  → Backend creates/updates user in PostgreSQL
  → Backend sets JWT cookie and redirects to UI
```

**SP Metadata endpoint:** `GET /api/v1/auth/saml/metadata` — returns XML metadata that you can import into your IdP.

#### SAML Group → Team Mapping

SAML groups from the assertion are automatically mapped to AppControl teams via the `saml_group_mappings` table:

```
AD Group "APP_PAYMENTS_OPERATORS"
  → AppControl team "Payments-Ops"
    → Permission: operate on "Paiements-SEPA"

AD Group "APP_PAYMENTS_ADMINS"
  → AppControl team "Payments-Admin"
    → Permission: manage on "Paiements-SEPA"

AD Group "APPCONTROL_ADMINS" (= SAML_ADMIN_GROUP)
  → role=admin (org admin, implicit owner on everything)
```

**Manage mappings via API:**

```bash
# List all group mappings
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/v1/saml/group-mappings

# Create a mapping
curl -X POST http://localhost:3000/api/v1/saml/group-mappings \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "saml_group": "CN=APP_PAYMENTS_OPERATORS,OU=Groups,DC=corp,DC=com",
    "team_id": "<team-uuid>",
    "default_role": "operator"
  }'

# Delete a mapping
curl -X DELETE http://localhost:3000/api/v1/saml/group-mappings/<mapping-id> \
  -H "Authorization: Bearer $TOKEN"
```

**Sync behavior on login:**
1. Extract group claims from the SAML assertion
2. For each group, look up `saml_group_mappings` to find the target team
3. Add user to matched teams (if not already a member)
4. Remove user from SAML-managed teams whose group is no longer in the assertion
5. This ensures team membership always reflects the current AD/LDAP state

---

## Backend (API Server)

The backend is the central API server. It is configured **exclusively via environment variables** — no config file is needed.

### Core

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APP_ENV` | No | `development` | `development`, `staging`, or `production`. In production, missing `JWT_SECRET` or `DATABASE_URL` causes a **fatal startup error**. |
| `PORT` | No | `3000` | HTTP listen port |
| `LOG_FORMAT` | No | `text` | `text` (human-readable) or `json` (structured JSON, recommended for production log aggregation) |
| `RUST_LOG` | No | `info` | Log level filter ([tracing-subscriber syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)). Examples: `info`, `appcontrol_backend=debug`, `appcontrol_backend=debug,tower_http=info` |

### Database

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | **Prod: Yes** | `postgresql://appcontrol:appcontrol@localhost:5432/appcontrol` | PostgreSQL 16 connection string. **Must** include `?sslmode=require` in production. |
| `DB_POOL_SIZE` | No | `20` | Maximum connections in the pool. Rule of thumb: 2-3x the number of CPU cores. |
| `DB_IDLE_TIMEOUT_SECS` | No | `600` | Close idle connections after N seconds. Prevents stale connections behind load balancers with idle timeouts (e.g., AWS ALB = 350s). |
| `DB_CONNECT_TIMEOUT_SECS` | No | `30` | Timeout for acquiring a connection from the pool. If the pool is exhausted, requests wait up to this duration before returning an error. |

**Migrations** run automatically on backend startup — no manual step required.

### Authentication

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `JWT_SECRET` | **Prod: Yes** | `dev-secret-change-in-production` | JWT signing secret. **Must be >= 32 characters** in production. The backend will **panic** on startup if the secret is weak and `APP_ENV=production`. Generate a strong secret: `openssl rand -base64 48` |
| `JWT_ISSUER` | No | `appcontrol` | JWT `iss` claim value. Must match across all backend instances in a cluster. |

### High Availability

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `HA_MODE` | No | `false` | When `true`, rate limiting uses PostgreSQL instead of in-memory counters. **Enable when running multiple backend replicas** behind a load balancer. |

### Rate Limiting

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RATE_LIMIT_AUTH` | No | `10` | Authentication endpoints: max requests per IP per minute |
| `RATE_LIMIT_OPERATIONS` | No | `5` | Operation endpoints (start/stop/restart): max requests per user per minute |
| `RATE_LIMIT_READS` | No | `200` | Read endpoints (GET): max requests per user per minute |

### CORS

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CORS_ORIGINS` | **Prod: Yes** | _(permissive in dev)_ | Comma-separated allowed origins. Example: `https://appcontrol.example.com,https://admin.example.com`. In production, empty = **reject all** cross-origin requests. In development, empty = permissive. |

### Data Retention

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RETENTION_ACTION_LOG_DAYS` | No | `0` (unlimited) | Drop `action_log` entries older than N days. The action_log is append-only; this controls background cleanup only. |
| `RETENTION_CHECK_EVENTS_DAYS` | No | `0` (unlimited) | Drop `check_events` partitions older than N days. **Recommended: `90`** for production (3 months of health check history). |

### Graceful Shutdown

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SHUTDOWN_TIMEOUT_SECS` | No | `30` | Time (seconds) to wait for in-flight requests during shutdown. Should be slightly less than Kubernetes `terminationGracePeriodSeconds`. |

### Security Headers

The backend automatically adds the following security headers to every response (no configuration needed):

| Header | Value |
|--------|-------|
| `X-Frame-Options` | `DENY` |
| `X-Content-Type-Options` | `nosniff` |
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains` |
| `X-XSS-Protection` | `1; mode=block` |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self'; ...` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=()` |

### Backend API Endpoints Summary

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/health` | GET | No | Health check (returns `{"status":"ok"}`) |
| `/ready` | GET | No | Readiness probe (checks DB connection) |
| `/metrics` | GET | No | Prometheus metrics |
| `/api/v1/auth/login` | POST | No | Email+password login (dev mode only) |
| `/api/v1/auth/dev-login` | POST | No | Email-only dev login (dev mode only) |
| `/api/v1/auth/oidc/login` | GET | No | OIDC login redirect |
| `/api/v1/auth/oidc/callback` | GET | No | OIDC callback |
| `/api/v1/auth/saml/login` | GET | No | SAML login redirect |
| `/api/v1/auth/saml/acs` | POST | No | SAML Assertion Consumer Service |
| `/api/v1/auth/saml/metadata` | GET | No | SAML SP metadata (XML) |
| `/api/v1/enroll` | POST | Token | Agent/gateway enrollment |
| `/api/v1/apps/**` | ALL | JWT | Application CRUD, operations |
| `/api/v1/teams/**` | ALL | JWT | Team management |
| `/api/v1/api-keys/**` | ALL | JWT | API key management |
| `/ws` | GET | JWT | Client WebSocket (real-time events) |
| `/ws/gateway` | GET | - | Gateway WebSocket (internal) |

### Redis — Removed

Redis is **no longer used** by AppControl. Token revocation and rate limiting are handled entirely by PostgreSQL:

- **Token revocation:** Revoked token fingerprints are stored in the `revoked_tokens` table with automatic expiry cleanup.
- **Rate limiting:** Counters use PostgreSQL (when `HA_MODE=true`) or in-memory (single instance).

The `REDIS_URL` environment variable is no longer recognized.

---

## Gateway

The gateway is the network relay between the backend and agents. It is deployed close to the agents (same network zone) and maintains persistent WebSocket connections in both directions.

### Configuration Source

The gateway is configured via **YAML file** (`/etc/appcontrol/gateway.yaml`) with environment variable overrides. If no config file exists, all values come from environment variables with sensible defaults.

**Config file search order:**
1. Path specified via `--config` CLI flag
2. `/etc/appcontrol/gateway.yaml` (Linux/macOS)
3. `%PROGRAMDATA%\AppControl\config\gateway.yaml` (Windows)

### Full YAML Reference

```yaml
# /etc/appcontrol/gateway.yaml
gateway:
  id: "gateway-prd-01"            # Unique gateway identifier (used to generate deterministic UUID)
  zone: "PRD"                     # Network zone label (PRD, DR, DMZ, etc.)
  listen_addr: "0.0.0.0"         # Bind address for agent connections
  listen_port: 4443              # Listen port for agent WebSocket connections

backend:
  url: "ws://backend:3000/ws/gateway"  # Backend WebSocket URL (MUST end with /ws/gateway)
  reconnect_interval_secs: 5           # Seconds to wait before reconnecting after disconnection

tls:                              # Omit entire section to disable mTLS (dev only!)
  enabled: true
  cert_file: "/etc/appcontrol/tls/gateway.crt"   # Gateway server certificate (PEM)
  key_file: "/etc/appcontrol/tls/gateway.key"     # Gateway private key (PEM)
  ca_file: "/etc/appcontrol/tls/ca.crt"           # CA certificate for verifying agent client certs
```

### Environment Variable Overrides

Environment variables take precedence over YAML values.

| Variable | YAML Path | Default | Description |
|----------|-----------|---------|-------------|
| `GATEWAY_ID` | `gateway.id` | `gateway-01` | Unique identifier. Used to generate a deterministic UUID v5. |
| `GATEWAY_ZONE` | `gateway.zone` | `default` | Network zone label (for display and filtering). |
| `LISTEN_ADDR` | `gateway.listen_addr` | `0.0.0.0` | Bind address |
| `LISTEN_PORT` | `gateway.listen_port` | `4443` | Listen port for agent WebSocket connections |
| `BACKEND_URL` | `backend.url` | `ws://localhost:3000/ws/gateway` | Backend WebSocket URL. **Must** end with `/ws/gateway`. |
| `BACKEND_RECONNECT_SECS` | `backend.reconnect_interval_secs` | `5` | Reconnect interval in seconds |
| `TLS_ENABLED` | `tls.enabled` | `false` | Enable mTLS (`true` or `1`) |
| `TLS_CERT_FILE` | `tls.cert_file` | - | Gateway server certificate path (PEM) |
| `TLS_KEY_FILE` | `tls.key_file` | - | Gateway private key path (PEM) |
| `TLS_CA_FILE` | `tls.ca_file` | - | CA certificate path for agent client cert verification (PEM) |
| `RUST_LOG` | - | `appcontrol_gateway=debug` | Log level filter |

### Gateway Network Architecture

The gateway does **NOT** connect to PostgreSQL or any database. Its only network requirements are:

| Direction | Target | Protocol | Port |
|-----------|--------|----------|------|
| **Outbound** | Backend | WebSocket (ws:// or wss://) | 3000 |
| **Inbound** | Agents | WebSocket (ws:// or wss://) | 4443 |

This allows gateways to be deployed in **isolated network zones** (DMZ, remote sites, air-gapped networks) without database access.

### Gateway Behaviors

- **Auto-reconnect:** If the backend connection drops, the gateway reconnects every `reconnect_interval_secs` seconds.
- **Agent re-announce:** When the backend connection is restored, the gateway re-announces all currently connected agents.
- **Agent rate limiting:** Built-in per-agent rate limiting to prevent a rogue agent from flooding the backend.
- **Enrollment proxy:** The gateway exposes `POST /enroll` to proxy enrollment requests from agents that don't have mTLS certificates yet.
- **Health endpoint:** `GET /health` returns `ok agents=N backend=connected|disconnected buffer_msgs=N buffer_bytes=N`.

### Gateway Windows Service

On Windows, the gateway can run as a Windows service:

```powershell
# Install as a Windows service
appcontrol-gateway.exe service install --config C:\ProgramData\AppControl\config\gateway.yaml

# Remove the service
appcontrol-gateway.exe service uninstall
```

On Linux, use systemd (see [Agent systemd section](#agent-as-a-systemd-service) for a similar unit file template).

---

## Agent

The agent runs on every monitored host. It executes health checks, start/stop commands, and reports status to the gateway.

### Configuration Source

The agent is configured via **YAML file** (`/etc/appcontrol/agent.yaml`) with environment variable overrides. If no config file exists, all values come from environment variables with sensible defaults.

**Config file search order:**
1. Path specified via `--config` CLI flag
2. `/etc/appcontrol/agent.yaml` (Linux/macOS)
3. `%PROGRAMDATA%\AppControl\config\agent.yaml` (Windows)

### Full YAML Reference

```yaml
# /etc/appcontrol/agent.yaml
agent:
  id: "auto"                     # "auto" = deterministic UUID v5 from hostname
                                 # Or set a fixed UUID: "550e8400-e29b-41d4-a716-446655440000"

gateway:
  # Simple setup — single gateway:
  url: "wss://gateway.example.com:4443/ws"

  # Recommended — multiple gateways with failover:
  urls:
    - "wss://gateway-prd-01.example.com:4443/ws"
    - "wss://gateway-prd-02.example.com:4443/ws"
  failover_strategy: "ordered"     # "ordered" = try in list order; "round-robin" = rotate
  primary_retry_secs: 300          # Attempt to return to the first (primary) gateway every 5 min
  reconnect_interval_secs: 10     # Wait between reconnection attempts

tls:                              # Omit entire section to disable mTLS (dev only!)
  enabled: true
  cert_file: "/etc/appcontrol/tls/agent.crt"   # Agent client certificate (PEM)
  key_file: "/etc/appcontrol/tls/agent.key"     # Agent private key (PEM)
  ca_file: "/etc/appcontrol/tls/ca.crt"         # CA for verifying the gateway server cert

labels:                           # Custom labels for filtering/grouping in the UI
  environment: "production"
  datacenter: "dc-paris-01"
  os: "rhel8"
  team: "platform"

log_level: "appcontrol_agent=info"  # tracing-subscriber filter syntax
```

### Environment Variable Overrides

| Variable | YAML Path | Default | Description |
|----------|-----------|---------|-------------|
| `AGENT_ID` | `agent.id` | `auto` | Agent ID. `auto` = deterministic UUID v5 from hostname. |
| `GATEWAY_URL` | `gateway.url` | `ws://localhost:4443/ws` | Single gateway URL |
| `GATEWAY_URLS` | `gateway.urls` | - | Comma-separated list of gateway URLs for failover. Example: `wss://gw1:4443/ws,wss://gw2:4443/ws` |
| `GATEWAY_RECONNECT_SECS` | `gateway.reconnect_interval_secs` | `10` | Reconnect interval in seconds |
| `TLS_ENABLED` | `tls.enabled` | `false` | Enable mTLS (`true` or `1`) |
| `TLS_CERT_FILE` | `tls.cert_file` | - | Client certificate path (PEM) |
| `TLS_KEY_FILE` | `tls.key_file` | - | Private key path (PEM) |
| `TLS_CA_FILE` | `tls.ca_file` | - | CA certificate path (PEM) |
| `RUST_LOG` | - | `appcontrol_agent=debug` | Log level filter |

### Agent Network Requirements

The agent only makes **outbound** connections:

| Direction | Target | Protocol | Port |
|-----------|--------|----------|------|
| **Outbound** | Gateway | WebSocket (wss://) | 4443 |

- **No inbound ports needed** — firewall-friendly.
- **No direct connection** to backend or database.
- Works behind NAT, corporate firewalls, and proxies.

### Agent Filesystem

| Path (Linux) | Path (Windows) | Purpose |
|-------------|----------------|---------|
| `/etc/appcontrol/agent.yaml` | `%PROGRAMDATA%\AppControl\config\agent.yaml` | Configuration file |
| `/etc/appcontrol/tls/` | `%PROGRAMDATA%\AppControl\config\tls\` | TLS certificates |
| `/var/lib/appcontrol/buffer-{agent-id}` | `%PROGRAMDATA%\AppControl\buffer-{agent-id}` | Offline message buffer (sled embedded DB) |

### Process Detachment

**Critical design rule:** Processes started by the agent **MUST survive agent crash or restart.** The agent uses double-fork + setsid (Unix) to ensure:

1. The child process is reparented to init/systemd (PID 1)
2. The child has its own session (setsid)
3. Agent crash, restart, or upgrade does not kill managed processes

### Offline Buffer

When the gateway connection is lost, the agent buffers messages in a local [sled](https://docs.rs/sled/) embedded database. When connectivity is restored, buffered messages are replayed in order. The buffer path is `/var/lib/appcontrol/buffer-{agent-id}`.

### Agent Failover

When multiple gateway URLs are configured:

- **`ordered` strategy:** Try gateways in list order. First available wins. Periodically retry the primary (first in list) every `primary_retry_secs` to return to the preferred gateway when it recovers.
- **`round-robin` strategy:** Rotate through gateways on each reconnection attempt.

### Agent as a systemd Service

```ini
# /etc/systemd/system/appcontrol-agent.service
[Unit]
Description=AppControl Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/appcontrol-agent --config /etc/appcontrol/agent.yaml
Restart=always
RestartSec=10
User=appcontrol
Group=appcontrol

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ReadWritePaths=/var/lib/appcontrol

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now appcontrol-agent
```

### Agent Windows Service

On Windows, the agent can run as a Windows service:

```powershell
appcontrol-agent.exe service install --config C:\ProgramData\AppControl\config\agent.yaml
appcontrol-agent.exe service uninstall
```

---

## Frontend

The frontend is a React SPA served by nginx. Configuration is minimal.

### Nginx Reverse Proxy

The frontend container bundles nginx which serves the static React build and proxies API/WebSocket requests to the backend.

| URL Pattern | Proxied To | Purpose |
|-------------|-----------|---------|
| `/api/**` | `http://backend:3000/api/**` | REST API |
| `/ws` | `http://backend:3000/ws` | Client WebSocket |
| `/**` | Static files (SPA fallback to `index.html`) | React SPA |

### Security Headers

Nginx adds the same security headers as the backend (defense in depth):

- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`
- `Content-Security-Policy: default-src 'self'; ...`

### Static Asset Caching

Static assets (`.js`, `.css`, `.png`, `.woff2`, etc.) are cached for 1 year with `Cache-Control: public, immutable`. Vite generates hashed filenames, so cache busting is automatic on each build.

---

## CLI (appctl)

The CLI is configured via **environment variables** and **command-line flags**.

| Variable | CLI Flag | Default | Description |
|----------|----------|---------|-------------|
| `APPCONTROL_URL` | `--url` | `http://localhost:3000` | Backend API URL |
| `APPCONTROL_API_KEY` | `--api-key` | - | API key for authentication (format: `ac_...`) |

### Exit Codes

Designed for scheduler integration (Control-M, AutoSys, Dollar Universe, TWS):

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Operation failed |
| `2` | Timeout |
| `3` | Authentication error |
| `4` | Resource not found |
| `5` | Permission denied |

### CLI Usage Examples

```bash
# Configure
export APPCONTROL_URL=https://appcontrol.example.com
export APPCONTROL_API_KEY=ac_xK9m2pQ...

# List applications
appctl list

# Start an application (waits for completion, 2min timeout)
appctl start $APP_ID --wait --timeout 120

# Check status
appctl status $APP_ID

# Stop application (reverse DAG order)
appctl stop $APP_ID --wait

# Restart failed branch only
appctl start-branch $APP_ID --component $COMPONENT_ID --wait

# Run diagnostics
appctl diagnose $APP_ID --level 1   # Health
appctl diagnose $APP_ID --level 2   # Integrity
appctl diagnose $APP_ID --level 3   # Infrastructure
```

---

## TLS / mTLS Certificate Configuration

### Certificate Chain

```
AppControl CA (self-signed or enterprise PKI)
├── Gateway server certificate
│   CN=appcontrol-gateway
│   SAN: DNS:gateway.example.com, DNS:*.gateway.internal
└── Agent client certificates (one per agent)
    CN=agent-{hostname}
```

### Three Deployment Modes

#### Mode 1: Enterprise PKI (Recommended)

Use your organization's existing PKI. Provide the CA, gateway cert, and agent certs.

```yaml
# Gateway config
tls:
  enabled: true
  cert_file: "/path/to/enterprise-gateway.crt"
  key_file: "/path/to/enterprise-gateway.key"
  ca_file: "/path/to/enterprise-ca.crt"    # Your corporate CA

# Agent config
tls:
  enabled: true
  cert_file: "/path/to/enterprise-agent.crt"
  key_file: "/path/to/enterprise-agent.key"
  ca_file: "/path/to/enterprise-ca.crt"    # Same corporate CA
```

#### Mode 2: Auto-PKI (Zero-Config)

AppControl can auto-generate a CA per organization on first startup. Agents then enroll via the enrollment API to obtain their certificates. This eliminates manual certificate management.

The backend auto-initializes PKI (CA) for organizations that don't have one. Agents call `POST /api/v1/enroll` (proxied through the gateway) to obtain their client certificates.

#### Mode 3: cert-manager (Kubernetes)

Automate certificate issuance and renewal in Kubernetes. See [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md#step-2-tls-certificates) for full cert-manager setup.

#### Mode 4: Manual / Self-Signed (Development)

Generate certificates manually with OpenSSL:

```bash
# 1. Create CA
openssl genrsa -out ca.key 4096
openssl req -new -x509 -key ca.key -out ca.crt -days 3650 -subj "/CN=AppControl CA"

# 2. Generate gateway certificate
openssl genrsa -out gateway.key 2048
openssl req -new -key gateway.key -out gateway.csr -subj "/CN=appcontrol-gateway"
openssl x509 -req -in gateway.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out gateway.crt -days 365

# 3. Generate agent certificate (repeat per agent)
openssl genrsa -out agent-host01.key 2048
openssl req -new -key agent-host01.key -out agent-host01.csr -subj "/CN=agent-host01"
openssl x509 -req -in agent-host01.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out agent-host01.crt -days 365
```

### Certificate Verification Flow

```
Agent connects to Gateway:
  1. Gateway presents its server cert → Agent verifies against ca_file ✓
  2. Agent presents its client cert   → Gateway verifies against ca_file ✓ (mTLS)
  3. Gateway computes SHA-256 fingerprint of agent cert for audit logging
  4. Connection accepted → WebSocket upgrade → Agent registers with ID + hostname
```

---

## Docker Compose Reference

### Release Stack (Pre-Built Images)

```bash
# Start with latest release
docker compose -f docker/docker-compose.release.yaml up -d

# Start with a specific version
APPCONTROL_VERSION=0.2.0 docker compose -f docker/docker-compose.release.yaml up -d
```

**Services and ports:**

| Service | Image | Port | Description |
|---------|-------|------|-------------|
| `postgres` | `postgres:16-alpine` | 5432 | PostgreSQL database |
| `backend` | `ghcr.io/fredericcarre/appcontrol-backend` | 3000 | API server |
| `frontend` | `ghcr.io/fredericcarre/appcontrol-frontend` | 8080 | React SPA (nginx) |
| `gateway` | `ghcr.io/fredericcarre/appcontrol-gateway` | 4443 | Agent relay |

**Default environment variables in docker-compose.release.yaml:**

```yaml
postgres:
  POSTGRES_DB: appcontrol
  POSTGRES_USER: appcontrol
  POSTGRES_PASSWORD: appcontrol_dev

backend:
  DATABASE_URL: postgres://appcontrol:appcontrol_dev@postgres:5432/appcontrol
  PORT: "3000"
  JWT_SECRET: dev-secret-change-in-production     # CHANGE for production!
  RUST_LOG: info,appcontrol_backend=debug

gateway:
  LISTEN_ADDR: "0.0.0.0"
  LISTEN_PORT: "4443"
  BACKEND_URL: ws://backend:3000/ws/gateway
  RUST_LOG: info,appcontrol_gateway=debug
```

### Dev Stack (Build from Source)

```bash
# Infrastructure only (PostgreSQL)
docker compose -f docker/docker-compose.dev.yaml up -d

# Full stack (builds from Dockerfiles)
docker compose -f docker/docker-compose.yaml up -d --build
```

---

## Production Checklist

### Backend
- [ ] `APP_ENV=production`
- [ ] `JWT_SECRET` set to a strong random value (>= 32 chars): `openssl rand -base64 48`
- [ ] `DATABASE_URL` points to managed PostgreSQL 16 with `?sslmode=require`
- [ ] `CORS_ORIGINS` set to your frontend URL(s)
- [ ] `LOG_FORMAT=json` for log aggregation (ELK, Datadog, Loki)
- [ ] `DB_POOL_SIZE` tuned (default 20 is fine for most deployments)
- [ ] `HA_MODE=true` if running multiple backend replicas
- [ ] `RETENTION_CHECK_EVENTS_DAYS=90` (or appropriate retention for your compliance needs)
- [ ] OIDC or SAML configured for SSO
- [ ] `SHUTDOWN_TIMEOUT_SECS` < Kubernetes `terminationGracePeriodSeconds`

### Gateway
- [ ] `tls.enabled=true` with valid certificates
- [ ] `gateway.zone` set correctly (PRD, DR, DMZ, etc.)
- [ ] `backend.url` points to the correct backend WebSocket endpoint (ends with `/ws/gateway`)
- [ ] Deployed in the same network zone as the agents it serves
- [ ] Health check configured: `GET /health`

### Agent
- [ ] `tls.enabled=true` with valid client certificate
- [ ] `gateway.urls` set with failover targets (at least 2 gateways recommended)
- [ ] Labels configured for inventory and filtering
- [ ] `/var/lib/appcontrol/` directory exists and is writable (for offline buffer)
- [ ] systemd unit file (Linux) or Windows service installed for auto-restart
- [ ] `failover_strategy` set (`ordered` for active/standby, `round-robin` for load distribution)

### Database
- [ ] PostgreSQL 16 (no other database supported)
- [ ] SSL enabled (`sslmode=require` in connection string)
- [ ] Regular backups configured
- [ ] Connection pooling (PgBouncer) if > 100 agents

### Security
- [ ] All traffic encrypted (mTLS for agent↔gateway, TLS for everything else)
- [ ] JWT secret rotated periodically
- [ ] API keys have expiry dates set
- [ ] SAML assertions are signed (`SAML_WANT_ASSERTIONS_SIGNED=true`)
- [ ] CORS origins restricted to your frontend domain only
- [ ] No dev endpoints accessible (`APP_ENV=production` disables `/auth/login` and `/auth/dev-login`)
