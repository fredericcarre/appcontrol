# AppControl Architecture

## Network Topology

```
                        +------------------+
                        |    Browser /     |
                        |    appctl CLI    |
                        +--------+---------+
                                 |
                                 | HTTPS :8080 (nginx)
                                 | or HTTP :3000 (direct API)
                                 v
                   +----------------------------+
                   |        Frontend (nginx)    |
                   |        :8080               |
                   |                            |
                   |  /api/*  --> backend:3000  |
                   |  /ws     --> backend:3000  |
                   |  /*      --> SPA (React)   |
                   +-------------+--------------+
                                 |
                    HTTP/WS (internal network)
                                 |
         +-----------------------+-----------------------+
         |                                               |
         v                                               v
+------------------+                          +-------------------+
|    Backend API   |                          |    PostgreSQL 16  |
|    :3000         |                          |    :5432          |
|                  +------- sqlx pool ------->+                   |
|  REST API        |                          +-------------------+
|  WebSocket /ws   |
|  (JWT auth)      +------- redis client --->+-------------------+
|                  |                          |    Redis 7        |
+--------+---------+                          |    :6379          |
         |                                    +-------------------+
         | WebSocket /ws
         | (internal, gateway connects here)
         |
+--------+---------+
|    Gateway       |
|    :4443         |
|                  |
|  Accepts agent   |
|  WebSocket (mTLS)|
|  Routes messages |
|  backend <-> agent|
+---+-----------+--+
    |           |
    | WSS :4443 | WSS :4443
    |  (mTLS)   |  (mTLS)
    v           v
+--------+ +--------+ +--------+
| Agent  | | Agent  | | Agent  |
| srv-01 | | srv-02 | | srv-N  |
|        | |        | |        |
| No     | | No     | | No     |
| inbound| | inbound| | inbound|
| port   | | port   | | port   |
+--------+ +--------+ +--------+
```

## Component Roles

| Component | Role | Inbound Port | Config Source |
|-----------|------|-------------|---------------|
| **Frontend** | SPA + nginx reverse proxy | `:8080` | `nginx.conf` |
| **Backend** | REST API + WebSocket hub + FSM engine | `:3000` (env `PORT`) | Env vars only |
| **Gateway** | Agent connection multiplexer | `:4443` (env `LISTEN_PORT`) | YAML + env var overrides |
| **Agent** | Local process manager + health checker | None (client only) | YAML + env var overrides |
| **CLI** | CLI client for automation/schedulers | None (client only) | Env vars + CLI args |
| **PostgreSQL** | Persistent storage | `:5432` | Standard |
| **Redis** | Cache + pub/sub for real-time events | `:6379` | Standard |

## Communication Protocols

```
Browser ---HTTPS---> nginx:8080 ---HTTP---> backend:3000
                                     (proxy_pass /api/* and /ws)

appctl  ---HTTP(S)---> backend:3000
                       (direct API calls, no nginx needed)

Gateway ---WebSocket---> backend:3000/ws
          (internal, persistent connection, reconnect on failure)

Agent ---WSS (mTLS)---> gateway:4443/ws
         (outbound only, reconnect with exponential backoff 1s..60s)
```

## Connection Details

### Agent --> Gateway (WSS, mTLS)
- Agent initiates outbound WebSocket connection to gateway
- No inbound port needed on the agent (firewall-friendly)
- mTLS: agent presents its certificate, gateway verifies it
- WSS on port 443/4443 passes through most corporate firewalls
  (indistinguishable from HTTPS traffic)
- Automatic reconnect with exponential backoff (1s, 2s, 4s... max 60s)
- Offline buffer: agent stores messages locally if gateway is unreachable

### Gateway --> Backend (WebSocket)
- Gateway connects to backend WebSocket endpoint
- Routes messages bidirectionally: agents <--> backend
- Reconnect on disconnect with configurable interval
- In-memory message buffer during backend unavailability

### Frontend --> Backend (via nginx)
- nginx proxies `/api/*` to `backend:3000/api/`
- nginx proxies `/ws` to `backend:3000/ws` (WebSocket upgrade)
- All other routes serve the React SPA (client-side routing)

### CLI --> Backend (HTTP)
- Direct HTTP calls to backend REST API
- No nginx/frontend needed
- Configured via `APPCONTROL_URL` env var or `--url` CLI arg
- Exit codes designed for scheduler integration (0=OK, 1=Fail, 2=Timeout)

## Environment Variables

### Backend
| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | HTTP listen port |
| `DATABASE_URL` | `postgresql://appcontrol:appcontrol@localhost:5432/appcontrol` | PostgreSQL connection string |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection string |
| `JWT_SECRET` | `dev-secret-change-in-production` | JWT signing secret |
| `JWT_ISSUER` | `appcontrol` | JWT issuer claim |

### Gateway
| Variable | Default | Description |
|----------|---------|-------------|
| `LISTEN_ADDR` | `0.0.0.0` | Bind address |
| `LISTEN_PORT` | `4443` | Bind port for agent connections |
| `BACKEND_URL` | `ws://localhost:3000/ws` | Backend WebSocket URL |
| `GATEWAY_ID` | `gateway-01` | Gateway identifier |
| `GATEWAY_ZONE` | `default` | Zone label (PRD, DR, etc.) |
| `BACKEND_RECONNECT_SECS` | `5` | Reconnect interval to backend |

### Agent
| Variable | Default | Description |
|----------|---------|-------------|
| `GATEWAY_URL` | `ws://localhost:4443/ws` | Gateway WebSocket URL |
| `AGENT_ID` | `auto` (hostname-based UUID) | Agent identifier |
| `GATEWAY_RECONNECT_SECS` | `10` | Reconnect interval |
| `TLS_ENABLED` | `false` | Enable mTLS |
| `TLS_CERT_FILE` | - | Agent certificate path |
| `TLS_KEY_FILE` | - | Agent private key path |
| `TLS_CA_FILE` | - | CA certificate path |

### CLI
| Variable | Default | Description |
|----------|---------|-------------|
| `APPCONTROL_URL` | `http://localhost:3000` | Backend API URL |
| `APPCONTROL_API_KEY` | - | API key for authentication |

## Docker Compose Ports

| Service | Internal Port | External Port | Purpose |
|---------|--------------|---------------|---------|
| postgres | 5432 | 5432 | Database |
| redis | 6379 | 6379 | Cache |
| backend | 3000 | 3000 | REST API + WebSocket |
| frontend | 8080 | 8080 | Web UI |
| gateway | 4443 | 4443 | Agent connections |
