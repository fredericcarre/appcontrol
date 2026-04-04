# AppControl

[![Release](https://img.shields.io/github/v/release/xcomponent/appcontrol-release?display_name=tag&sort=semver)](https://github.com/xcomponent/appcontrol-release/releases/latest)

**Operational mastery and IT system resilience.** AppControl maps your applications as dependency graphs (DAGs), monitors component health via distributed agents, orchestrates sequenced start/stop/restart operations, manages DR site failover, and provides full DORA-compliant audit trails.

> AppControl is **not** a scheduler. It integrates with existing schedulers (Control-M, AutoSys, Dollar Universe, TWS) via REST API and CLI.

---

## Key Features

| Feature | Description |
|---------|-------------|
| **Dependency Maps** | Model applications as DAGs with strong/weak dependencies, visualized in React Flow |
| **Sequenced Operations** | Start, stop, restart in correct DAG order with parallel execution within levels |
| **3-Level Diagnostics** | Health (process alive?), Integrity (data consistent?), Infrastructure (OS/prereqs OK?) |
| **Error Branch Restart** | Detect failed subgraph, restart only affected components |
| **DR Switchover** | 6-phase site failover with rollback at any phase |
| **RBAC** | 5 permission levels (view < operate < edit < manage < owner), teams, share links |
| **Audit Trail** | DORA-compliant append-only logs for every action, state transition, and config change |
| **Scheduler Integration** | REST API + `appctl` CLI for Control-M, AutoSys, TWS, Dollar Universe |
| **Distributed Agents** | Rust agents with process detachment (survives agent crash), offline buffering, mTLS |
| **Realtime UI** | React SPA with WebSocket live updates, weather-style dashboards |

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         Frontend (React 18)                         │
│             TypeScript · Vite · Tailwind · shadcn/ui                │
│                     React Flow · React Query                        │
└──────────────────────┬───────────────────────┬───────────────────────┘
                  REST │                   WS  │
┌──────────────────────▼───────────────────────▼───────────────────────┐
│                       Backend (Rust + Axum)                          │
│       FSM Engine · DAG Sequencer · RBAC · Switchover · Reports      │
├──────────────────┬───────────────────────────────────────────────────┤
│  PostgreSQL 16   │  or SQLite (standalone)     JWT RS256 Auth        │
└──────────────────┘                             └─────────────────────┘
                                  │
┌─────────────────────────────────▼────────────────────────────────────┐
│                      Gateway (Rust + Axum)                           │
│                 WebSocket relay · mTLS · Routing                     │
└───────────────────┬──────────────────────┬───────────────────────────┘
                    │                      │
         ┌──────────▼──────────┐ ┌─────────▼──────────┐
         │   Agent (Rust)      │ │   Agent (Rust)      │
         │ Health checks       │ │ Health checks       │
         │ Process detachment  │ │ Process detachment  │
         │ Offline buffer      │ │ Offline buffer      │
         └─────────────────────┘ └─────────────────────┘
```

---

## Quick Start

### Option 1 — Docker Compose (Linux/macOS)

```bash
# Download the compose file
gh release download --repo xcomponent/appcontrol-release --pattern 'docker-compose.release.yaml'

# Start the full stack
APPCONTROL_VERSION=1.4.2 docker compose -f docker-compose.release.yaml up -d

# Verify
curl http://localhost:3000/health   # → {"status":"ok"}

# Open the UI
open http://localhost:8080
```

**Login:** `admin@localhost` / `admin`

### Option 2 — Standalone (Windows/Linux, no Docker)

No database to install, no Docker required. One script does everything.

```powershell
mkdir AppControl; cd AppControl

# Download the standalone script (visible at the repo root)
Invoke-WebRequest -Uri "https://github.com/xcomponent/appcontrol-release/raw/main/appcontrol.ps1" -OutFile appcontrol.ps1

# Install (downloads binaries + frontend)
.\appcontrol.ps1 install

# Start the backend
.\appcontrol.ps1 start

# Add your first site (creates gateway + agent, handles enrollment)
.\appcontrol.ps1 add-site Production

# Add more sites
.\appcontrol.ps1 add-site DR-Site
```

**Login:** `admin@localhost` / `admin`  
**Web UI:** http://localhost:3000

Other commands: `stop`, `status`, `upgrade`, `logs [file]`, `help`

> Works on Windows PowerShell 5.1+ and PowerShell Core 6+ (Linux/macOS).

### Option 3 — CLI only

```bash
# Linux (amd64)
gh release download --repo xcomponent/appcontrol-release --pattern 'appctl-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl-linux-amd64 && mv /usr/local/bin/appctl-linux-amd64 /usr/local/bin/appctl

# macOS (Apple Silicon)
gh release download --repo xcomponent/appcontrol-release --pattern 'appctl-darwin-arm64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl-darwin-arm64 && mv /usr/local/bin/appctl-darwin-arm64 /usr/local/bin/appctl

# Windows
gh release download --repo xcomponent/appcontrol-release --pattern 'appctl-windows-amd64.exe' --dir $env:LOCALAPPDATA\appcontrol
```

```bash
export APPCONTROL_URL=http://localhost:3000

appctl login --email admin@localhost --password admin
appctl start my-app --wait --timeout 120
appctl status my-app --format table
appctl diagnose my-app --level 2
appctl switchover my-app --target-site lyon --mode FULL --wait
```

---

## Docker Images

All images are available on GitHub Container Registry:

```bash
docker pull ghcr.io/xcomponent/appcontrol-backend:latest
docker pull ghcr.io/xcomponent/appcontrol-frontend:latest
docker pull ghcr.io/xcomponent/appcontrol-gateway:latest
docker pull ghcr.io/xcomponent/appcontrol-agent:latest
docker pull ghcr.io/xcomponent/appcontrol-init-certs:latest
```

## Release Assets

Each release includes:

| Asset | Description |
|-------|-------------|
| `appctl-{os}-{arch}[.exe]` | CLI binary (Linux, macOS, Windows) |
| `appcontrol-agent-{os}-{arch}[.exe]` | Agent binary |
| `appcontrol-backend-{os}-{arch}[.exe]` | Backend API server (PostgreSQL) |
| `appcontrol-backend-sqlite-{os}-{arch}[.exe]` | Backend API server (SQLite standalone) |
| `appcontrol-gateway-{os}-{arch}[.exe]` | Gateway binary |
| `appcontrol.ps1` | Standalone deployment script (Windows PS 5.1+ / Linux pwsh) |
| `docker-compose.release.yaml` | Docker Compose for the full stack |
| `appcontrol-docs-scripts.zip` | Documentation, scripts, and examples |
| `appcontrol-*.tgz` | Helm chart (OpenShift compatible) |
| `checksums-sha256.txt` | SHA-256 checksums |

## Example Maps

Ready-to-import application maps in the docs zip:

| Example | Components | Highlights |
|---------|:----------:|------------|
| Three-Tier Web App | 7 | Strong/weak deps, DB replication, batch processing |
| Microservices E-Commerce | 12 | API gateway, message broker, service-per-DB pattern |
| Core Banking System | 9 | DR switchover, Control-M integration, DORA compliance |

## Documentation

Full documentation is included in `appcontrol-docs-scripts.zip`:

- **[QUICKSTART.md](docs/QUICKSTART.md)** — Getting started guide
- **[USER_GUIDE.md](docs/USER_GUIDE.md)** — Complete user guide with screenshots
- **[WINDOWS_DEPLOYMENT.md](docs/WINDOWS_DEPLOYMENT.md)** — Windows deployment guide
- **[AGENT_INSTALLATION.md](docs/AGENT_INSTALLATION.md)** — Agent installation on all platforms
- **[CONFIGURATION.md](docs/CONFIGURATION.md)** — All configuration options
- **[AZURE_GATEWAY.md](docs/AZURE_GATEWAY.md)** — Azure gateway deployment
- **[PRODUCTION_DEPLOYMENT.md](docs/PRODUCTION_DEPLOYMENT.md)** — Production hardening guide

## Scripts

| Script | Description |
|--------|-------------|
| **`appcontrol.ps1`** | **Unified standalone deployment** (install, start, stop, add-site, upgrade) |
| `scripts/deploy-windows.ps1` | Deploy Windows services (PostgreSQL mode) |
| `scripts/install-agent-windows.ps1` | Install agent as Windows service |
| `scripts/deploy-azure-gateway.sh` | Deploy gateway on Azure |

---

## License

Copyright (c) 2024-2026 XComponent SAS. All rights reserved.

This software is provided as pre-compiled binaries for evaluation and production use.
Redistribution, reverse engineering, and offering as a managed service are prohibited
without a commercial license.

Contact: support@xcomponent.com
