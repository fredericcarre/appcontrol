# Deploying the AppControl Azure Gateway

This guide deploys a combined **Gateway + Agent** container on Azure Container Instances (ACI) with **Managed Identity** to orchestrate Azure VM start/stop operations from AppControl.

The gateway connects back to the AppControl backend running on OpenShift (see [OPENSHIFT.md](OPENSHIFT.md)).

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  OpenShift (Backend)                                 │
│  wss://appcontrol-api-xxx.apps.sandbox.../ws/gateway │
└──────────────────────┬───────────────────────────────┘
                       │ WebSocket
                       │
┌──────────────────────▼───────────────────────────────┐
│  Azure Container Instance                             │
│  ┌─────────────────────────────────────────────────┐ │
│  │  supervisord                                     │ │
│  │  ├── appcontrol-gateway  (connects to backend)   │ │
│  │  └── appcontrol-agent    (connects to gateway)   │ │
│  │                                                  │ │
│  │  Azure CLI (logged in via Managed Identity)      │ │
│  └─────────────────────────────────────────────────┘ │
│                                                       │
│  User-Assigned Managed Identity                       │
│  Role: Virtual Machine Contributor                    │
└──────────────────────┬───────────────────────────────┘
                       │
           ┌───────────┼───────────┐
           │           │           │
      ┌────▼───┐  ┌────▼───┐  ┌───▼────┐
      │ VM: DB │  │ VM: App│  │ VM: Web│
      └────────┘  └────────┘  └────────┘
```

### How It Works

1. The **gateway** connects to the OpenShift backend via WebSocket
2. The **agent** connects to the local gateway (127.0.0.1:4443)
3. When AppControl sends a start/stop command, the agent executes `az vm start` or `az vm deallocate`
4. Azure CLI authenticates using the container's **Managed Identity** — no passwords stored
5. The agent reports VM power state back via `az vm get-instance-view`

## Prerequisites

| Tool | Install |
|------|---------|
| Azure CLI | [Install](https://docs.microsoft.com/cli/azure/install-azure-cli) |
| Azure subscription | [Create free account](https://azure.microsoft.com/free/) |
| AppControl backend | Deployed on OpenShift (see [OPENSHIFT.md](OPENSHIFT.md)) |

## Step 1: Login to Azure

```bash
az login
az account set --subscription "My Subscription"
```

## Step 2: Deploy

### Quick deploy

```bash
cd azure/

BACKEND_URL="wss://appcontrol-api-user-dev.apps.sandbox-m2.ll9k.p1.openshiftapps.com/ws/gateway" \
./deploy.sh
```

### Full configuration

```bash
export BACKEND_URL="wss://appcontrol-api-user-dev.apps.sandbox.openshiftapps.com/ws/gateway"
export RESOURCE_GROUP="appcontrol-rg"
export LOCATION="westeurope"
export GATEWAY_ID="azure-gateway-01"
export GATEWAY_ZONE="azure-westeurope"
export VM_RESOURCE_GROUP="my-vms-rg"

./deploy.sh
```

The script will:
1. Create a resource group
2. Create a User-Assigned Managed Identity
3. Assign "Virtual Machine Contributor" role on the target resource group
4. Deploy the container instance with the identity attached
5. The container starts, logs in with Managed Identity, and connects to the backend

## Step 3: Verify Connection

### Check container logs

```bash
./deploy.sh --status
```

Expected output:
```
[INFO] Logging in with Azure Managed Identity...
[OK]   Logged in with user-assigned identity: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
[OK]   Subscription set to: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
[INFO] Gateway ID:   azure-gateway-01
[INFO] Backend URL:  wss://appcontrol-api-xxx.apps.sandbox.../ws/gateway
[INFO] Starting supervisord...
```

### Verify in AppControl UI

1. Open the AppControl frontend URL
2. Go to **Gateways** — you should see `azure-gateway-01` connected
3. Go to **Agents** — you should see the agent registered

## Step 4: Create an Application

### Via the UI

1. Go to **Applications** → **Create**
2. Add components with Azure VM commands:

| Field | Example Value |
|-------|---------------|
| Name | `database-vm` |
| Host / Agent | `azure-gateway-01` (the agent ID) |
| Check command | `az vm get-instance-view -g my-rg -n db-server --query "instanceView.statuses[?starts_with(code,'PowerState/')].displayStatus" -o tsv \| grep -q 'VM running'` |
| Start command | `az vm start -g my-rg -n db-server --no-wait` |
| Stop command | `az vm deallocate -g my-rg -n db-server --no-wait` |
| Check interval | 30 seconds |

### Via the API

```bash
curl -X POST https://<backend-url>/api/v1/applications \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d @example-app.json
```

See `azure/example-app.json` for a full example with dependencies (DB → App → Web).

## Step 5: Test VM Operations

Once the application is created with its components:

1. The agent will start running health checks (every 30s)
2. VM power state will appear on the topology map:
   - **Green** = VM running
   - **Red** = VM deallocated/stopped
   - **Grey** = Unknown
3. Click a component → **Start** or **Stop** to trigger the Azure command
4. The DAG ensures correct sequencing: DB starts before App, App before Web

## Azure VM Commands Reference

### Power state check

```bash
# Returns "VM running", "VM deallocated", "VM stopped", etc.
az vm get-instance-view -g <resource-group> -n <vm-name> \
  --query "instanceView.statuses[?starts_with(code,'PowerState/')].displayStatus" -o tsv
```

### Start a VM

```bash
az vm start -g <resource-group> -n <vm-name> --no-wait
```

### Stop (deallocate) a VM

Deallocate releases compute resources (no charges). Use `stop` to keep the VM allocated.

```bash
az vm deallocate -g <resource-group> -n <vm-name> --no-wait
```

### Restart a VM

```bash
az vm restart -g <resource-group> -n <vm-name> --no-wait
```

### List all VMs in a resource group

```bash
az vm list -g <resource-group> --show-details \
  --query "[].{name:name, state:powerState, size:hardwareProfile.vmSize}" -o table
```

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `BACKEND_URL` | **(required)** | Backend WebSocket URL (`wss://...`) |
| `RESOURCE_GROUP` | `appcontrol-rg` | Azure resource group |
| `LOCATION` | `westeurope` | Azure region |
| `CONTAINER_NAME` | `appcontrol-gateway` | ACI container name |
| `IDENTITY_NAME` | `appcontrol-identity` | Managed Identity name |
| `GATEWAY_ID` | `azure-gateway-01` | Gateway identifier |
| `GATEWAY_ZONE` | `azure` | Gateway zone label |
| `VM_RESOURCE_GROUP` | same as `RESOURCE_GROUP` | Resource group for VM role assignment |
| `IMAGE` | `ghcr.io/fredericcarre/appcontrol-azure-gateway:latest` | Docker image |
| `ACR_NAME` | (empty) | Azure Container Registry name (for `--build`) |
| `CPU` | `1` | CPU cores |
| `MEMORY` | `1.5` | Memory in GB |

## Operations

### View logs

```bash
az container logs --resource-group appcontrol-rg --name appcontrol-gateway --tail 100
```

### Restart the container

```bash
az container restart --resource-group appcontrol-rg --name appcontrol-gateway
```

### Exec into the container

```bash
az container exec --resource-group appcontrol-rg --name appcontrol-gateway --exec-command /bin/bash
```

### Delete everything

```bash
./deploy.sh --delete
```

## Advanced: Using Azure Container Registry (ACR)

If you want to build and store the image in your own ACR:

```bash
# Create ACR (one time)
az acr create --resource-group appcontrol-rg --name myappcontrolacr --sku Basic

# Build and deploy
ACR_NAME=myappcontrolacr \
BACKEND_URL="wss://..." \
./deploy.sh --build
```

## Advanced: Multiple Zones

Deploy multiple gateways for different Azure regions or environments:

```bash
# Production West Europe
RESOURCE_GROUP=appcontrol-prod-we \
LOCATION=westeurope \
GATEWAY_ID=azure-we-01 \
GATEWAY_ZONE=azure-westeurope \
VM_RESOURCE_GROUP=prod-vms-we \
BACKEND_URL="wss://..." \
./deploy.sh

# Production East US
RESOURCE_GROUP=appcontrol-prod-eus \
LOCATION=eastus \
GATEWAY_ID=azure-eus-01 \
GATEWAY_ZONE=azure-eastus \
VM_RESOURCE_GROUP=prod-vms-eus \
BACKEND_URL="wss://..." \
./deploy.sh
```

Each gateway registers with a different zone, and the AppControl backend sees all of them.

## Troubleshooting

### Container won't start

Check events:
```bash
az container show --resource-group appcontrol-rg --name appcontrol-gateway \
  --query "containers[0].instanceView.events" -o table
```

### Managed Identity login fails

Verify the identity is assigned:
```bash
az container show --resource-group appcontrol-rg --name appcontrol-gateway \
  --query identity.userAssignedIdentities -o json
```

### "az vm" commands fail with permission error

Check role assignments:
```bash
PRINCIPAL_ID=$(az identity show -g appcontrol-rg -n appcontrol-identity --query principalId -o tsv)
az role assignment list --assignee $PRINCIPAL_ID -o table
```

Ensure "Virtual Machine Contributor" is assigned on the correct resource group.

### Gateway can't connect to backend

- Verify the backend route is accessible: `curl https://<backend-route>/health`
- Check that the WebSocket endpoint works: the URL must end with `/ws/gateway`
- Ensure CORS allows the gateway origin (or use the backend's internal URL)

### Container keeps restarting

Check logs for crash reason:
```bash
az container logs --resource-group appcontrol-rg --name appcontrol-gateway
```

Common causes:
- Invalid `BACKEND_URL`
- Network connectivity issues
- Image pull failures (if using private registry)
