<!--
Release-only tail appended to README.en.md by
.github/workflows/release.yaml when mirroring the dev repo to
xcomponent/appcontrol-release. Replaces the dev-only "Quickstart" /
"License" sections with their release equivalents (binary install,
no dev workflow, XComponent license). The narrative above the
RELEASE-CUT marker in README.en.md is shared with the dev repo and
is not duplicated here.
-->

## Quick Start

### Option 1 — Docker Compose (Linux / macOS)

```bash
gh release download --repo xcomponent/appcontrol-release --pattern 'docker-compose.release.yaml'
APPCONTROL_VERSION=latest docker compose -f docker-compose.release.yaml up -d
open http://localhost:8080
```

Login: `admin@localhost` / `admin`.

### Option 2 — Standalone PowerShell (Windows / Linux, no Docker)

```powershell
mkdir AppControl; cd AppControl
Invoke-WebRequest -Uri "https://github.com/xcomponent/appcontrol-release/raw/main/appcontrol.ps1" -OutFile appcontrol.ps1

.\appcontrol.ps1 install
.\appcontrol.ps1 start
.\appcontrol.ps1 add-site Production
.\appcontrol.ps1 add-site DR-Site
```

Works on Windows PowerShell 5.1+ and PowerShell Core 6+ (Linux/macOS).

### Option 3 — CLI only (`appctl`)

```bash
gh release download --repo xcomponent/appcontrol-release --pattern 'appctl-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl-linux-amd64 && mv /usr/local/bin/appctl-linux-amd64 /usr/local/bin/appctl

export APPCONTROL_URL=http://localhost:3000
appctl login --email admin@localhost --password admin
appctl start core-banking --wait --timeout 120
appctl switchover core-banking --target-site lyon --mode FULL --wait
```

## Docker Images

```bash
docker pull ghcr.io/xcomponent/appcontrol-backend:latest
docker pull ghcr.io/xcomponent/appcontrol-frontend:latest
docker pull ghcr.io/xcomponent/appcontrol-gateway:latest
docker pull ghcr.io/xcomponent/appcontrol-agent:latest
docker pull ghcr.io/xcomponent/appcontrol-init-certs:latest
```

## Documentation

Full documentation is included in `appcontrol-docs-scripts.zip`:

- **[QUICKSTART.md](docs/QUICKSTART.md)** — Getting started
- **[USER_GUIDE.md](docs/USER_GUIDE.md)** — Complete user guide with screenshots
- **[WINDOWS_DEPLOYMENT.md](docs/WINDOWS_DEPLOYMENT.md)** — Windows deployment
- **[AGENT_INSTALLATION.md](docs/AGENT_INSTALLATION.md)** — Agent installation (all platforms)
- **[CONFIGURATION.md](docs/CONFIGURATION.md)** — Configuration options
- **[AZURE_GATEWAY.md](docs/AZURE_GATEWAY.md)** — Azure gateway deployment
- **[PRODUCTION_DEPLOYMENT.md](docs/PRODUCTION_DEPLOYMENT.md)** — Production hardening

## Contact

`support@xcomponent.com` — describe your case in three lines: an application name, the scheduler in place, the DR horizon to cover. Reply within 48h.

## License

Copyright (c) 2024-2026 XComponent SAS. All rights reserved.

This software is provided as pre-compiled binaries for evaluation and production use. Redistribution, reverse engineering, and offering as a managed service are prohibited without a commercial license.

Contact: `support@xcomponent.com` for licensing.
