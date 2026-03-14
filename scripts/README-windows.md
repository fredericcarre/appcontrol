# AppControl - Windows Deployment

## Quick Start (Docker)

### Prerequisites
- Windows 10/11 or Windows Server 2019+
- [Docker Desktop](https://www.docker.com/products/docker-desktop/) installed and running

### Steps

1. **Open PowerShell as Administrator**

2. **Run the deployment script:**
   ```powershell
   cd C:\path\to\appcontrol\scripts
   .\deploy-windows.ps1
   ```

3. **First run creates `appcontrol-config.json`** - Edit it to configure:
   - Database password
   - Gateways (add your sites)

4. **Run again to deploy:**
   ```powershell
   .\deploy-windows.ps1
   ```

5. **Access the UI:** http://localhost:8080

## Configuration

### Adding Gateways

Edit `appcontrol-config.json`:

```json
{
  "gateways": [
    {
      "name": "production-paris",
      "zone": "production",
      "port": 8443,
      "backend_url": "ws://localhost:3000/ws/gateway"
    },
    {
      "name": "dr-london",
      "zone": "disaster-recovery",
      "port": 8444,
      "backend_url": "ws://localhost:3000/ws/gateway"
    }
  ]
}
```

Each gateway needs:
- **name**: Unique identifier (used in agent config)
- **zone**: Logical grouping (production, disaster-recovery, dev, etc.)
- **port**: Port for agents to connect (firewall this!)
- **backend_url**: URL to backend WebSocket

### Database Options

#### Option 1: Docker PostgreSQL (default)
No extra setup - PostgreSQL runs in a container.

#### Option 2: External PostgreSQL
```json
{
  "database": {
    "host": "your-postgres-server.example.com",
    "port": 5432,
    "name": "appcontrol",
    "user": "appcontrol",
    "password": "your-secure-password"
  }
}
```

#### Option 3: Azure PostgreSQL
```json
{
  "database": {
    "host": "myserver.postgres.database.azure.com",
    "port": 5432,
    "name": "appcontrol",
    "user": "appcontrol@myserver",
    "password": "your-azure-password"
  }
}
```

## Commands

### Start services
```powershell
docker compose -f docker-compose.windows.yaml up -d
```

### Stop services
```powershell
docker compose -f docker-compose.windows.yaml down
```

### View logs
```powershell
docker compose -f docker-compose.windows.yaml logs -f
```

### Update to latest version
```powershell
docker compose -f docker-compose.windows.yaml pull
docker compose -f docker-compose.windows.yaml up -d
```

## Agent Installation

On each machine to monitor:

1. Download the agent:
   ```powershell
   Invoke-WebRequest -Uri "https://github.com/fredericcarre/appcontrol/releases/latest/download/appcontrol-agent-windows-amd64.exe" -OutFile "C:\Program Files\AppControl\appcontrol-agent.exe"
   ```

2. Create config `C:\Program Files\AppControl\agent.yaml`:
   ```yaml
   agent_id: auto
   gateway_url: wss://your-gateway-host:8443
   # For mTLS (recommended for production):
   # cert_file: C:\Program Files\AppControl\certs\agent.crt
   # key_file: C:\Program Files\AppControl\certs\agent.key
   # ca_file: C:\Program Files\AppControl\certs\ca.crt
   ```

3. Install as Windows Service:
   ```powershell
   sc.exe create AppControlAgent binPath="C:\Program Files\AppControl\appcontrol-agent.exe --config C:\Program Files\AppControl\agent.yaml" start=auto
   sc.exe start AppControlAgent
   ```

## Troubleshooting

### Docker not starting
- Ensure Hyper-V is enabled
- Restart Docker Desktop
- Check Windows Firewall

### Database connection errors
- Verify PostgreSQL is running: `docker ps`
- Check credentials in config
- Test connection: `psql -h localhost -U appcontrol -d appcontrol`

### Gateway not connecting
- Check backend logs: `docker logs appcontrol-backend`
- Verify backend_url is correct
- Check firewall allows the gateway port
