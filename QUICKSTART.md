# Quick Start - AppControl v4

Get AppControl running in under 5 minutes.

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Docker Desktop | 4.x | [docker.com](https://www.docker.com/products/docker-desktop/) |
| Docker Compose | v2+ | Included with Docker Desktop |
| Git | 2.x | `brew install git` (macOS) |

For local development (optional):

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.77+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 22+ | `brew install node@22` (macOS) |
| PostgreSQL client | 16 | `brew install libpq` (macOS) |

## 1. Clone and start

```bash
git clone https://github.com/fredericcarre/appcontrol.git
cd appcontrol

# Start full stack (backend, frontend, gateway, postgres, redis)
docker compose -f docker/docker-compose.yaml up -d
```

Wait for all services to be healthy:

```bash
docker compose -f docker/docker-compose.yaml ps
```

| Service | Port | URL |
|---------|------|-----|
| Frontend | 8080 | http://localhost:8080 |
| Backend API | 3000 | http://localhost:3000 |
| Gateway (WSS) | 4443 | wss://localhost:4443 |
| PostgreSQL | 5432 | `postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol` |
| Redis | 6379 | `redis://localhost:6379` |

## 2. Verify the stack

```bash
# API health check
curl http://localhost:3000/health

# Should return: {"status":"ok"}
```

## 3. Load example data

AppControl ships with example application maps in `examples/`. Import one:

```bash
# Import the 3-tier web app example
curl -X POST http://localhost:3000/api/v1/apps/import \
  -H "Content-Type: application/json" \
  -d @examples/three-tier-webapp.json

# Import the microservices example
curl -X POST http://localhost:3000/api/v1/apps/import \
  -H "Content-Type: application/json" \
  -d @examples/microservices-ecommerce.json
```

Then open http://localhost:8080 to see the maps.

## 4. Register your first agent

On the host you want to monitor:

```bash
# Download and install the agent
docker run -d --name appcontrol-agent \
  -e GATEWAY_URL=wss://YOUR_SERVER:4443 \
  -e AGENT_NAME=my-first-agent \
  -e AGENT_LABELS='{"env":"dev","zone":"local"}' \
  appcontrol/agent:latest
```

Or run natively for development:

```bash
cd crates/agent
cargo run -- \
  --gateway-url wss://localhost:4443 \
  --name dev-agent \
  --labels env=dev,zone=local
```

## 5. Create your first application map

### Via the UI

1. Open http://localhost:8080
2. Click **"New Application"**
3. Drag components onto the map canvas
4. Draw dependency edges between components
5. Configure check commands on each component
6. Click **"Save"**

### Via the API

```bash
# Create an application
APP_ID=$(curl -s -X POST http://localhost:3000/api/v1/apps \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My First App",
    "description": "A simple app with 2 components",
    "site_id": "YOUR_SITE_UUID"
  }' | jq -r '.id')

# Add a database component
DB_ID=$(curl -s -X POST http://localhost:3000/api/v1/apps/$APP_ID/components \
  -H "Content-Type: application/json" \
  -d '{
    "name": "PostgreSQL",
    "component_type": "database",
    "agent_id": "YOUR_AGENT_UUID",
    "check_cmd": "pg_isready -h localhost -p 5432",
    "start_cmd": "systemctl start postgresql",
    "stop_cmd": "systemctl stop postgresql",
    "check_interval_secs": 30
  }' | jq -r '.id')

# Add a web server component
WEB_ID=$(curl -s -X POST http://localhost:3000/api/v1/apps/$APP_ID/components \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Nginx",
    "component_type": "webserver",
    "agent_id": "YOUR_AGENT_UUID",
    "check_cmd": "curl -sf http://localhost:80/health",
    "start_cmd": "systemctl start nginx",
    "stop_cmd": "systemctl stop nginx",
    "check_interval_secs": 30
  }' | jq -r '.id')

# Add dependency: Nginx depends on PostgreSQL
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/dependencies \
  -H "Content-Type: application/json" \
  -d "{\"from_component_id\": \"$WEB_ID\", \"to_component_id\": \"$DB_ID\"}"
```

## 6. Operate your application

```bash
# Start full application (respects DAG order)
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/start

# Check status
curl http://localhost:3000/api/v1/apps/$APP_ID/status

# Stop application (reverse DAG order)
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/stop

# Restart failed branch only
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/start-branch \
  -d '{"component_id": "FAILED_COMPONENT_UUID"}'
```

### Using the CLI

```bash
# Build the CLI
cargo build --release --bin appctl

# Start an app and wait for it
./target/release/appctl start $APP_ID --wait --timeout 120

# Get application status
./target/release/appctl status $APP_ID

# Run diagnostics
./target/release/appctl diagnose $APP_ID
```

## 7. Run diagnostics

AppControl provides 3 diagnostic levels:

```bash
# Level 1 (Health): Is the process alive?
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/diagnose \
  -d '{"level": 1}'

# Level 2 (Integrity): Is the data consistent?
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/diagnose \
  -d '{"level": 2}'

# Level 3 (Infrastructure): Is the OS/filesystem OK?
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/diagnose \
  -d '{"level": 3}'
```

## Troubleshooting

### Containers won't start

```bash
# Check logs
docker compose -f docker/docker-compose.yaml logs backend
docker compose -f docker/docker-compose.yaml logs postgres

# Reset everything
docker compose -f docker/docker-compose.yaml down -v
docker compose -f docker/docker-compose.yaml up -d
```

### Database migration issues

```bash
# Run migrations manually
DATABASE_URL=postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol \
  sqlx migrate run --source migrations/
```

### Agent can't connect to gateway

- Verify the gateway is running: `curl -k https://localhost:4443/health`
- Check TLS certificates in the agent logs
- Ensure the gateway URL is reachable from the agent host

## Next steps

- Read the [example maps](examples/) for real-world configurations
- Set up [DR switchover](docs/) for multi-site resilience
- Configure [OIDC/SAML authentication](docs/) for enterprise SSO
- Deploy to Kubernetes with the [Helm chart](helm/)
