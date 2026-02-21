# Quick Start - AppControl v4

Get AppControl running in under 5 minutes — no build required.

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Docker Desktop | 4.x | [docker.com](https://www.docker.com/products/docker-desktop/) |
| Docker Compose | v2+ | Included with Docker Desktop |

That's it. Everything else comes from pre-built images.

## 1. Download and start

```bash
# Grab the latest release compose file + examples
gh release download --repo fredericcarre/appcontrol --pattern 'docker-compose.release.yaml' --dir .
gh release download --repo fredericcarre/appcontrol --pattern 'examples.tar.gz' --dir . && tar xzf examples.tar.gz

# Or clone the repo (also gives you examples/, helm/, etc.)
git clone https://github.com/fredericcarre/appcontrol.git && cd appcontrol
```

Start the full stack from pre-built images:

```bash
# Latest release
docker compose -f docker/docker-compose.release.yaml up -d

# Or pin to a specific version
APPCONTROL_VERSION=0.2.0 docker compose -f docker/docker-compose.release.yaml up -d
```

Wait for all services to be healthy:

```bash
docker compose -f docker/docker-compose.release.yaml ps
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

## 4. Download the CLI

Download the pre-built `appctl` binary from the latest release:

```bash
# Linux (amd64)
gh release download --repo fredericcarre/appcontrol --pattern 'appctl-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl

# macOS (Apple Silicon)
gh release download --repo fredericcarre/appcontrol --pattern 'appctl-darwin-arm64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl

# macOS (Intel)
gh release download --repo fredericcarre/appcontrol --pattern 'appctl-darwin-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl
```

Configure the endpoint:

```bash
export APPCONTROL_URL=http://localhost:3000
```

## 5. Register your first agent

Download and run the agent from the release image:

```bash
docker run -d --name appcontrol-agent \
  --network host \
  ghcr.io/fredericcarre/appcontrol-agent:latest \
  --gateway-url wss://localhost:4443 \
  --name my-first-agent \
  --labels env=dev,zone=local
```

Or pull the agent binary directly:

```bash
gh release download --repo fredericcarre/appcontrol --pattern 'appcontrol-agent-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appcontrol-agent

appcontrol-agent \
  --gateway-url wss://YOUR_SERVER:4443 \
  --name prod-agent \
  --labels env=production,zone=PRD
```

## 6. Create your first application map

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

## 7. Operate your application

```bash
# Start full application (respects DAG order)
appctl start $APP_ID --wait --timeout 120

# Check status
appctl status $APP_ID

# Stop application (reverse DAG order)
appctl stop $APP_ID --wait

# Restart failed branch only
appctl start-branch $APP_ID --component $FAILED_COMPONENT_UUID --wait
```

Or use the API directly:

```bash
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/start
curl http://localhost:3000/api/v1/apps/$APP_ID/status
curl -X POST http://localhost:3000/api/v1/apps/$APP_ID/stop
```

## 8. Run diagnostics

AppControl provides 3 diagnostic levels:

```bash
# Level 1 (Health): Is the process alive?
appctl diagnose $APP_ID --level 1

# Level 2 (Integrity): Is the data consistent?
appctl diagnose $APP_ID --level 2

# Level 3 (Infrastructure): Is the OS/filesystem OK?
appctl diagnose $APP_ID --level 3
```

## Troubleshooting

### Containers won't start

```bash
# Check logs
docker compose -f docker/docker-compose.release.yaml logs backend
docker compose -f docker/docker-compose.release.yaml logs postgres

# Reset everything
docker compose -f docker/docker-compose.release.yaml down -v
docker compose -f docker/docker-compose.release.yaml up -d
```

### Database migration issues

Migrations run automatically on backend startup. If needed manually:

```bash
# Run migrations via the backend container
docker compose -f docker/docker-compose.release.yaml exec backend \
  sqlx migrate run --source /app/migrations
```

### Agent can't connect to gateway

- Verify the gateway is running: `curl -k https://localhost:4443/health`
- Check TLS certificates in the agent logs
- Ensure the gateway URL is reachable from the agent host

### Check release versions

```bash
# List all available releases
gh release list --repo fredericcarre/appcontrol

# See assets in a specific release
gh release view v0.2.0 --repo fredericcarre/appcontrol
```

---

## Building from source (for contributors)

If you want to build locally instead of using release images, see [docker/docker-compose.yaml](docker/docker-compose.yaml) which builds from source, and [docker/dev-setup.sh](docker/dev-setup.sh) for the full dev environment setup:

```bash
# Dev infrastructure only (PostgreSQL + Redis)
docker compose -f docker/docker-compose.dev.yaml up -d

# Build everything
./docker/dev-setup.sh

# Or use the build compose (builds from Dockerfiles)
docker compose -f docker/docker-compose.yaml up -d --build
```

## Next steps

- Read the [example maps](examples/) for real-world configurations
- Set up [DR switchover](docs/) for multi-site resilience
- Configure [OIDC/SAML authentication](docs/) for enterprise SSO
- Deploy to Kubernetes with the [Helm chart](helm/)
- See the [release procedure](RELEASE.md) for versioning and CI/CD
