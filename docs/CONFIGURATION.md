# Configuration Reference

Complete configuration reference for all AppControl components.

---

## Backend (API Server)

The backend is configured **exclusively via environment variables**. No config file.

### Core

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APP_ENV` | No | `development` | `development`, `staging`, or `production`. In production mode, missing `JWT_SECRET` or `DATABASE_URL` causes a fatal error. |
| `PORT` | No | `3000` | HTTP listen port |
| `LOG_FORMAT` | No | `text` | `text` (human-readable) or `json` (structured, recommended for production) |
| `RUST_LOG` | No | `info` | Log level filter (tracing-subscriber syntax: `info`, `appcontrol_backend=debug`, etc.) |

### Database

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | **Prod: Yes** | `postgresql://appcontrol:appcontrol@localhost:5432/appcontrol` | PostgreSQL connection string. Must include `?sslmode=require` for production. |
| `DB_POOL_SIZE` | No | `20` | Maximum database connections in the pool. Rule of thumb: 2-3x the number of CPU cores. |
| `DB_IDLE_TIMEOUT_SECS` | No | `600` | Close idle connections after this many seconds. Prevents stale connections behind load balancers with idle timeouts. |
| `DB_CONNECT_TIMEOUT_SECS` | No | `30` | Timeout for acquiring a connection from the pool. If the pool is exhausted, requests will wait up to this duration before returning an error. |

### Authentication

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `JWT_SECRET` | **Prod: Yes** | `dev-secret-change-in-production` | JWT RS256 signing secret. Must be >= 32 characters in production. Generate with: `openssl rand -base64 48` |
| `JWT_ISSUER` | No | `appcontrol` | JWT `iss` claim. Must match between backend instances. |

### OIDC (Optional)

Set `OIDC_DISCOVERY_URL` to enable OIDC authentication (Keycloak, Okta, Azure AD, etc.).

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OIDC_DISCOVERY_URL` | Yes (to enable) | - | OpenID Connect discovery URL (e.g., `https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration`) |
| `OIDC_CLIENT_ID` | Yes | - | Client ID registered with the OIDC provider |
| `OIDC_CLIENT_SECRET` | Yes | - | Client secret |
| `OIDC_REDIRECT_URI` | No | `/api/v1/auth/oidc/callback` | Redirect URI after authentication |
| `OIDC_SCOPES` | No | `openid,profile,email` | Comma-separated OIDC scopes |

### SAML 2.0 (Optional)

Set `SAML_IDP_SSO_URL` to enable SAML authentication (ADFS, Azure AD, Okta, etc.).

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SAML_IDP_SSO_URL` | Yes (to enable) | - | IdP SSO endpoint URL |
| `SAML_IDP_CERT` | Yes | - | IdP signing certificate (PEM, base64-encoded) |
| `SAML_SP_ENTITY_ID` | Yes | - | Service Provider entity ID (e.g., `https://appcontrol.example.com/saml`) |
| `SAML_SP_ACS_URL` | Yes | - | Assertion Consumer Service URL (e.g., `https://appcontrol.example.com/api/v1/auth/saml/acs`) |
| `SAML_GROUP_ATTRIBUTE` | No | `memberOf` | SAML attribute name containing group memberships |
| `SAML_EMAIL_ATTRIBUTE` | No | `email` | SAML attribute name for user email |
| `SAML_NAME_ATTRIBUTE` | No | `displayName` | SAML attribute name for display name |
| `SAML_ADMIN_GROUP` | No | - | SAML group name mapped to org admin role |
| `SAML_WANT_ASSERTIONS_SIGNED` | No | `true` | Require IdP to sign SAML assertions |

### Redis — Removed (Since Phase 10)

Redis is **no longer used** by AppControl. As of Phase 10, token revocation and rate limiting are handled entirely by PostgreSQL:

- **Token revocation:** Revoked token fingerprints are stored in the `revoked_tokens` PostgreSQL table with automatic expiry cleanup. No external cache needed.
- **Rate limiting:** Rate limit counters use PostgreSQL-backed storage, eliminating the previous in-memory-only limitation.

The `REDIS_URL` environment variable is no longer recognized. If you have an existing Redis instance from a prior version, it can be safely decommissioned.

### Rate Limiting

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RATE_LIMIT_AUTH` | No | `10` | Authentication endpoints: max requests per IP per minute |
| `RATE_LIMIT_OPERATIONS` | No | `5` | Operation endpoints (start/stop/restart): max requests per user per minute |
| `RATE_LIMIT_READS` | No | `200` | Read endpoints (GET): max requests per user per minute |

### CORS

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CORS_ORIGINS` | **Prod: Yes** | _(permissive in dev)_ | Comma-separated allowed origins. Example: `https://appcontrol.example.com,https://admin.example.com`. In production, empty = reject all cross-origin requests. |

### Data Retention

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RETENTION_ACTION_LOG_DAYS` | No | `0` (unlimited) | Drop action_log entries older than N days. Note: action_log is append-only; this controls background cleanup. |
| `RETENTION_CHECK_EVENTS_DAYS` | No | `0` (unlimited) | Drop check_events partitions older than N days. Recommended: `90` for production (3 months of health check history). |

### Graceful Shutdown

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SHUTDOWN_TIMEOUT_SECS` | No | `30` | Time to wait for in-flight requests to complete during shutdown. Should be slightly less than Kubernetes `terminationGracePeriodSeconds`. |

---

## Gateway

The gateway is configured via **YAML file** (`/etc/appcontrol/gateway.yaml`) with environment variable overrides.

### YAML Configuration

```yaml
gateway:
  id: "gateway-prd-01"          # Unique gateway identifier
  zone: "PRD"                   # Network zone label (PRD, DR, DMZ, etc.)
  listen_addr: "0.0.0.0"       # Bind address
  listen_port: 4443            # Listen port for agent WebSocket connections

backend:
  url: "ws://backend:3000/ws/gateway"  # Backend WebSocket URL
  reconnect_interval_secs: 5          # Reconnect delay after disconnection

tls:                            # Omit entire section to disable mTLS
  enabled: true
  cert_file: "/etc/appcontrol/tls/gateway.crt"   # Gateway server certificate
  key_file: "/etc/appcontrol/tls/gateway.key"     # Gateway private key
  ca_file: "/etc/appcontrol/tls/ca.crt"           # CA for verifying agent client certs
```

### Environment Variable Overrides

Environment variables take precedence over YAML values.

| Variable | YAML Path | Default | Description |
|----------|-----------|---------|-------------|
| `GATEWAY_ID` | `gateway.id` | `gateway-01` | Unique identifier |
| `GATEWAY_ZONE` | `gateway.zone` | `default` | Network zone |
| `LISTEN_ADDR` | `gateway.listen_addr` | `0.0.0.0` | Bind address |
| `LISTEN_PORT` | `gateway.listen_port` | `4443` | Listen port |
| `BACKEND_URL` | `backend.url` | `ws://localhost:3000/ws/gateway` | Backend WebSocket URL |
| `BACKEND_RECONNECT_SECS` | `backend.reconnect_interval_secs` | `5` | Reconnect interval |
| `TLS_ENABLED` | `tls.enabled` | `false` | Enable mTLS |
| `TLS_CERT_FILE` | `tls.cert_file` | - | Server certificate path |
| `TLS_KEY_FILE` | `tls.key_file` | - | Private key path |
| `TLS_CA_FILE` | `tls.ca_file` | - | CA certificate path (for client verification) |

### Network Architecture Note

The gateway does **NOT** connect to PostgreSQL. It only needs:
- Outbound WebSocket to the backend (`backend.url`)
- Inbound WebSocket from agents (on `listen_port`)

This means gateways can be deployed in isolated network zones (DMZ, remote sites) without database or cache access.

---

## Agent

The agent is configured via **YAML file** (`/etc/appcontrol/agent.yaml`) with environment variable overrides.

### YAML Configuration

```yaml
agent:
  id: "auto"                   # "auto" = deterministic UUID from hostname

gateway:
  # Single gateway (simple setup)
  url: "wss://gateway.example.com:4443/ws"

  # OR: Multiple gateways for failover (recommended)
  urls:
    - "wss://gateway-prd-01.example.com:4443/ws"
    - "wss://gateway-prd-02.example.com:4443/ws"
  failover_strategy: "ordered"   # "ordered" or "round-robin"
  primary_retry_secs: 300        # Try returning to first gateway every 5 min
  reconnect_interval_secs: 10   # Wait between reconnection attempts

tls:                            # Omit entire section to disable mTLS
  enabled: true
  cert_file: "/etc/appcontrol/tls/agent.crt"   # Agent client certificate
  key_file: "/etc/appcontrol/tls/agent.key"     # Agent private key
  ca_file: "/etc/appcontrol/tls/ca.crt"         # CA for verifying gateway server cert

labels:                         # Custom labels for filtering/grouping
  environment: "production"
  datacenter: "dc-paris-01"
  os: "rhel8"

log_level: "appcontrol_agent=info"  # tracing-subscriber filter syntax
```

### Environment Variable Overrides

| Variable | YAML Path | Default | Description |
|----------|-----------|---------|-------------|
| `AGENT_ID` | `agent.id` | `auto` | Agent ID. `auto` generates a deterministic UUID from hostname. |
| `GATEWAY_URL` | `gateway.url` | `ws://localhost:4443/ws` | Single gateway URL |
| `GATEWAY_URLS` | `gateway.urls` | - | Comma-separated list of gateway URLs for failover |
| `GATEWAY_RECONNECT_SECS` | `gateway.reconnect_interval_secs` | `10` | Reconnect interval |
| `TLS_ENABLED` | `tls.enabled` | `false` | Enable mTLS |
| `TLS_CERT_FILE` | `tls.cert_file` | - | Client certificate path |
| `TLS_KEY_FILE` | `tls.key_file` | - | Private key path |
| `TLS_CA_FILE` | `tls.ca_file` | - | CA certificate path |

### Agent Network Requirements

The agent only makes **outbound** connections:
- Outbound WebSocket (WSS on port 443 or 4443) to the gateway
- No inbound ports needed — firewall-friendly
- No direct connection to backend or database

### Agent Filesystem

| Path | Purpose |
|------|---------|
| `/etc/appcontrol/agent.yaml` | Configuration file |
| `/etc/appcontrol/tls/` | TLS certificates |
| `/var/lib/appcontrol/buffer-{agent-id}` | Offline message buffer (sled embedded DB) |

---

## CLI (appctl)

The CLI is configured via **environment variables** and **command-line flags**.

| Variable | CLI Flag | Default | Description |
|----------|----------|---------|-------------|
| `APPCONTROL_URL` | `--url` | `http://localhost:3000` | Backend API URL |
| `APPCONTROL_API_KEY` | `--api-key` | - | API key for authentication |

### Exit Codes (for scheduler integration)

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Operation failed |
| `2` | Timeout |
| `3` | Authentication error |
| `4` | Resource not found |
| `5` | Permission denied |

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

#### Mode 1: Client-Provided Certificates (Enterprise)

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

#### Mode 2: cert-manager (Kubernetes)

Automate certificate issuance and renewal in Kubernetes. See [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md#step-2-tls-certificates) for full cert-manager setup.

#### Mode 3: Manual / Self-Signed (Development)

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
  1. Gateway presents its server cert → Agent verifies against ca_file
  2. Agent presents its client cert   → Gateway verifies against ca_file (mTLS)
  3. Gateway computes SHA-256 fingerprint of agent cert for audit logging
  4. Connection accepted → WebSocket upgrade
```

---

## Production Checklist

### Backend
- [ ] `APP_ENV=production`
- [ ] `JWT_SECRET` set to a strong random value (>= 32 chars)
- [ ] `DATABASE_URL` points to managed PostgreSQL 16 with `?sslmode=require`
- [ ] `CORS_ORIGINS` set to your frontend URL(s)
- [ ] `LOG_FORMAT=json` for log aggregation
- [ ] `DB_POOL_SIZE` tuned (default 20 is good for most deployments)
- [ ] `RETENTION_CHECK_EVENTS_DAYS=90` (or appropriate retention)
- [ ] OIDC or SAML configured for SSO

### Gateway
- [ ] `tls.enabled=true` with valid certificates
- [ ] `gateway.zone` set correctly (PRD, DR, etc.)
- [ ] `backend.url` points to the correct backend WebSocket endpoint
- [ ] Deployed in the same network zone as the agents it serves

### Agent
- [ ] `tls.enabled=true` with valid client certificate
- [ ] `gateway.urls` set with failover targets
- [ ] Labels configured for inventory and filtering
- [ ] `/var/lib/appcontrol/` directory exists and is writable (for offline buffer)
- [ ] systemd unit file installed for auto-restart

### Redis — No Longer Used
- Redis was removed in Phase 10. Token revocation and rate limiting now use PostgreSQL.
- No Redis instance is required for any deployment scenario.
- If upgrading from a pre-Phase 10 deployment, the existing Redis instance can be safely removed.

---

## Minimal Development Setup

```bash
# Start PostgreSQL
docker run -d --name pg -p 5432:5432 \
  -e POSTGRES_USER=appcontrol \
  -e POSTGRES_PASSWORD=appcontrol \
  -e POSTGRES_DB=appcontrol \
  postgres:16

# Run backend (all defaults are safe for development)
cargo run -p appcontrol-backend

# Run gateway (no TLS for local dev)
cargo run -p appcontrol-gateway

# Run agent
cargo run -p appcontrol-agent
```

No TLS needed for local testing.
