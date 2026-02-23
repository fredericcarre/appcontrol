# Windows Deployment Guide

Run the full AppControl stack on Windows Server — backend, gateway, and agents
as native Windows services.

## Architecture

```
  Windows Server (backend + DB)         Windows Servers (agents)
  +---------------------------------+    +-------------------------+
  | PostgreSQL 16 (Windows/Docker)  |    | AppControlAgent service |
  | AppControlBackend service      |    |   -> gateway:4443 (mTLS)|
  | AppControlBackend service      |    +-------------------------+
  |   -> :3000 (API)               |    +-------------------------+
  | AppControlGateway service      |    | AppControlAgent service |
  |   -> :4443 (WSS)               |    |   -> gateway:4443 (mTLS)|
  +---------------------------------+    +-------------------------+
```

All three components (backend, gateway, agent) are single `.exe` files with
built-in Windows service support. No external service wrappers needed.

---

## 1. Prerequisites

| Requirement | Notes |
|-------------|-------|
| Windows Server 2019+ or Windows 10/11 | Any 64-bit edition |
| PostgreSQL 16 | [EDB installer](https://www.enterprisedb.com/downloads/postgres-postgresql-downloads) or Docker |
| Administrator access | Required for service installation |

---

## 2. Download binaries

```powershell
# Create directories
New-Item -ItemType Directory -Force -Path "$env:ProgramFiles\AppControl"
New-Item -ItemType Directory -Force -Path "$env:ProgramData\AppControl\config"

# Download from GitHub release
$version = "latest"
foreach ($bin in @("appcontrol-backend", "appcontrol-gateway", "appcontrol-agent", "appctl")) {
    gh release download $version --repo fredericcarre/appcontrol `
        --pattern "${bin}-windows-amd64.exe" `
        --dir "$env:ProgramFiles\AppControl"
    Rename-Item "$env:ProgramFiles\AppControl\${bin}-windows-amd64.exe" "${bin}.exe" -Force
}

# Add to system PATH
[Environment]::SetEnvironmentVariable(
    "PATH",
    "$env:PATH;$env:ProgramFiles\AppControl",
    "Machine"
)
```

---

## 3. Database setup

### Option A: PostgreSQL on Windows

```powershell
# After installing PostgreSQL via EDB installer:
& "C:\Program Files\PostgreSQL\16\bin\psql.exe" -U postgres -c "CREATE DATABASE appcontrol;"
& "C:\Program Files\PostgreSQL\16\bin\psql.exe" -U postgres -c "CREATE USER appcontrol WITH PASSWORD 'your_secure_password';"
& "C:\Program Files\PostgreSQL\16\bin\psql.exe" -U postgres -c "GRANT ALL PRIVILEGES ON DATABASE appcontrol TO appcontrol;"
```

### Option B: PostgreSQL in Docker

```powershell
docker run -d --name postgres `
    -e POSTGRES_DB=appcontrol `
    -e POSTGRES_USER=appcontrol `
    -e POSTGRES_PASSWORD=your_secure_password `
    -p 5432:5432 `
    postgres:16
```

---

## 4. Configure and install the backend

The backend reads all configuration from **environment variables**. Set them
as system-level variables so the Windows service can access them:

```powershell
# Required
[Environment]::SetEnvironmentVariable("DATABASE_URL", "postgres://appcontrol:your_secure_password@localhost:5432/appcontrol", "Machine")
[Environment]::SetEnvironmentVariable("JWT_SECRET", "$(New-Guid)-$(New-Guid)", "Machine")

# Optional
[Environment]::SetEnvironmentVariable("PORT", "3000", "Machine")
[Environment]::SetEnvironmentVariable("LOG_FORMAT", "json", "Machine")
[Environment]::SetEnvironmentVariable("CORS_ORIGINS", "http://localhost:8080", "Machine")
```

Install and start the service:

```powershell
# Install (run as Administrator)
appcontrol-backend.exe service install

# Start
sc.exe start AppControlBackend

# Verify
sc.exe query AppControlBackend
curl http://localhost:3000/healthz
```

---

## 5. Configure and install the gateway

Create the gateway configuration:

```powershell
@"
gateway:
  id: "gateway-01"
  zone: "default"
  listen_addr: "0.0.0.0"
  listen_port: 4443

backend:
  url: "ws://localhost:3000/ws/gateway"
  reconnect_interval_secs: 5

# TLS configuration (after PKI init and enrollment)
# tls:
#   enabled: true
#   cert_file: "C:\\ProgramData\\AppControl\\config\\tls\\gateway.crt"
#   key_file: "C:\\ProgramData\\AppControl\\config\\tls\\gateway.key"
#   ca_file: "C:\\ProgramData\\AppControl\\config\\tls\\ca.crt"
"@ | Out-File -Encoding UTF8 "$env:ProgramData\AppControl\config\gateway.yaml"
```

Install and start:

```powershell
appcontrol-gateway.exe service install --config "$env:ProgramData\AppControl\config\gateway.yaml"
sc.exe start AppControlGateway

# Verify
curl http://localhost:4443/health
```

---

## 6. Initialize PKI and enroll agents

### 6.1 Initialize PKI (once)

```powershell
appctl.exe pki init --org-name "My Company"
```

### 6.2 Create enrollment tokens

```powershell
# For agents
appctl.exe pki create-token --name "windows-servers" --max-uses 50 --scope agent

# For the gateway (if using mTLS)
appctl.exe pki create-token --name "gateway" --max-uses 1 --scope gateway
```

### 6.3 Enroll agents

On each Windows server where you want to deploy an agent:

```powershell
# Enroll (run as Administrator)
appcontrol-agent.exe --enroll https://gateway-host:4443 --token ac_enroll_...

# Install as service
appcontrol-agent.exe service install --config "$env:ProgramData\AppControl\config\agent.yaml"

# Start
sc.exe start AppControlAgent
```

---

## 7. Service management cheat sheet

| Action | Backend | Gateway | Agent |
|--------|---------|---------|-------|
| Install | `appcontrol-backend.exe service install` | `appcontrol-gateway.exe service install -c ...` | `appcontrol-agent.exe service install -c ...` |
| Start | `sc start AppControlBackend` | `sc start AppControlGateway` | `sc start AppControlAgent` |
| Stop | `sc stop AppControlBackend` | `sc stop AppControlGateway` | `sc stop AppControlAgent` |
| Status | `sc query AppControlBackend` | `sc query AppControlGateway` | `sc query AppControlAgent` |
| Uninstall | `appcontrol-backend.exe service uninstall` | `appcontrol-gateway.exe service uninstall` | `appcontrol-agent.exe service uninstall` |
| Restart | `Restart-Service AppControlBackend` | `Restart-Service AppControlGateway` | `Restart-Service AppControlAgent` |

All services are set to **AutoStart** by default (they start on boot).

---

## 8. Firewall rules

```powershell
# Backend API (internal, or expose to UI clients)
New-NetFirewallRule -DisplayName "AppControl Backend" -Direction Inbound -LocalPort 3000 -Protocol TCP -Action Allow

# Gateway (agents connect here)
New-NetFirewallRule -DisplayName "AppControl Gateway" -Direction Inbound -LocalPort 4443 -Protocol TCP -Action Allow
```

---

## 9. Logs and monitoring

### Event Viewer

All services log to the Windows Application event log:

```powershell
# View recent backend logs
Get-WinEvent -FilterHashtable @{LogName='Application'; ProviderName='AppControlBackend'} -MaxEvents 50

# View agent logs
Get-WinEvent -FilterHashtable @{LogName='Application'; ProviderName='AppControlAgent'} -MaxEvents 50
```

### Prometheus metrics

The backend exposes metrics at `http://localhost:3000/metrics` (Prometheus format).

### Health checks

```powershell
# Backend
curl http://localhost:3000/healthz

# Gateway
curl http://localhost:4443/health
```

---

## 10. Troubleshooting

### Service won't start

```powershell
# Check service status
sc.exe query AppControlBackend
sc.exe query AppControlGateway

# Check event log for errors
Get-WinEvent -FilterHashtable @{LogName='System'; Level=2} -MaxEvents 20

# Run in foreground to see errors directly
appcontrol-backend.exe
appcontrol-gateway.exe --config "$env:ProgramData\AppControl\config\gateway.yaml"
appcontrol-agent.exe --config "$env:ProgramData\AppControl\config\agent.yaml"
```

### Database connection fails

```powershell
# Verify PostgreSQL is running
sc.exe query postgresql-x64-16

# Test connection
& "C:\Program Files\PostgreSQL\16\bin\psql.exe" -U appcontrol -d appcontrol -c "SELECT 1;"

# Check DATABASE_URL is set as system variable
[Environment]::GetEnvironmentVariable("DATABASE_URL", "Machine")
```

### Agent can't reach gateway

```powershell
# Test connectivity
Test-NetConnection -ComputerName gateway-host -Port 4443

# Check Windows Firewall
Get-NetFirewallRule | Where-Object { $_.LocalPort -eq 4443 }
```

---

## 11. Directory structure (Windows)

```
C:\Program Files\AppControl\
    appcontrol-backend.exe
    appcontrol-gateway.exe
    appcontrol-agent.exe
    appctl.exe

C:\ProgramData\AppControl\
    config\
        gateway.yaml
        agent.yaml          (auto-generated by enrollment)
        tls\
            agent.crt
            agent.key
            gateway.crt
            gateway.key
            ca.crt
    buffer-<agent-id>\      (offline buffer, sled DB)
```
