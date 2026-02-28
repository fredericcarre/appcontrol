# Deploying AppControl on Red Hat OpenShift Developer Sandbox

This guide deploys AppControl (Backend, Frontend, PostgreSQL) on the **free OpenShift Developer Sandbox** at [console.redhat.com/openshift/sandbox](https://console.redhat.com/openshift/sandbox).

The gateway and agent run separately — typically on an Azure Container Instance near your workloads. See [AZURE_GATEWAY.md](AZURE_GATEWAY.md) for that part.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  OpenShift Developer Sandbox                        │
│                                                     │
│  ┌───────────┐  ┌───────────┐  ┌────────────────┐  │
│  │ Frontend  │──│  Backend  │──│  PostgreSQL 16 │  │
│  │  (nginx)  │  │  (Axum)   │  │  (RHEL image)  │  │
│  └─────┬─────┘  └─────┬─────┘  └────────────────┘  │
│        │              │                              │
│   Route (TLS)    Route (TLS)                        │
│        │              │                              │
└────────┼──────────────┼──────────────────────────────┘
         │              │
    Users (browser)     │
                        │  wss:// (WebSocket)
                        │
              ┌─────────▼──────────┐
              │  Azure Container   │
              │  Gateway + Agent   │
              │  (Managed Identity)│
              └────────────────────┘
                    │
              az vm start/stop
```

## Prerequisites

| Tool | Install |
|------|---------|
| `oc` CLI | [Download](https://mirror.openshift.com/pub/openshift-v4/clients/ocp/latest/) |
| Red Hat account | [Create free account](https://sso.redhat.com) |
| OpenShift Sandbox | [Start sandbox](https://console.redhat.com/openshift/sandbox) |

## Step 1: Get Your Sandbox

1. Go to [console.redhat.com/openshift/sandbox](https://console.redhat.com/openshift/sandbox)
2. Click **Start your Sandbox for free**
3. Verify your account (phone number required)
4. Wait for the sandbox to provision (~30 seconds)

## Step 2: Login with `oc` CLI

1. In the OpenShift web console, click your username (top right) → **Copy login command**
2. Click **Display Token**
3. Copy the `oc login` command and run it:

```bash
oc login --token=sha256~xxxxxxxxxxxx --server=https://api.sandbox-m2.ll9k.p1.openshiftapps.com:6443
```

4. Verify:

```bash
oc whoami
oc project
```

## Step 3: Deploy AppControl

### Quick deploy (auto-generates passwords)

```bash
cd openshift/
./deploy.sh
```

### Custom configuration

```bash
export POSTGRES_PASSWORD="my-secure-password"
export JWT_SECRET="$(openssl rand -base64 48)"
export SEED_ADMIN_EMAIL="admin@mycompany.com"
export SEED_ADMIN_DISPLAY_NAME="Platform Admin"
export SEED_ORG_NAME="My Company"
export SEED_ORG_SLUG="mycompany"

./deploy.sh
```

### Using a specific image tag

```bash
export IMAGE_TAG="v4.0.0"
./deploy.sh
```

The script will:
1. Create secrets (PostgreSQL credentials, JWT key)
2. Deploy PostgreSQL 16 with persistent storage
3. Deploy the Backend (runs migrations automatically)
4. Deploy the Frontend (nginx with API proxy)
5. Create OpenShift Routes with TLS edge termination
6. Print the URLs and credentials

## Step 4: Access AppControl

The script prints the URLs at the end. Example:

```
Frontend:  https://appcontrol-username-dev.apps.sandbox-m2.ll9k.p1.openshiftapps.com
Backend:   https://appcontrol-api-username-dev.apps.sandbox-m2.ll9k.p1.openshiftapps.com
```

Login with the `SEED_ADMIN_EMAIL` you configured (default: `admin@localhost`).
Leave the password empty in dev mode.

## Step 5: Connect the Azure Gateway

After deploying the Azure gateway (see [AZURE_GATEWAY.md](AZURE_GATEWAY.md)), point it to the backend WebSocket URL:

```
BACKEND_URL=wss://appcontrol-api-<project>.apps.<cluster>/ws/gateway
```

The deploy script prints this URL at the end.

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_DB` | `appcontrol` | Database name |
| `POSTGRES_USER` | `appcontrol` | Database user |
| `POSTGRES_PASSWORD` | auto-generated | Database password |
| `JWT_SECRET` | auto-generated | JWT signing key |
| `IMAGE_TAG` | `latest` | Docker image tag |
| `IMAGE_REGISTRY` | `ghcr.io/fredericcarre` | Image registry |
| `SEED_ADMIN_EMAIL` | `admin@localhost` | Initial admin email |
| `SEED_ADMIN_DISPLAY_NAME` | `Admin` | Initial admin name |
| `SEED_ORG_NAME` | `Default Organization` | Organization name |
| `SEED_ORG_SLUG` | `default` | Organization slug |
| `ROUTE_HOST` | auto-derived | Frontend hostname |
| `BACKEND_ROUTE_HOST` | auto-derived | Backend hostname |

## Operations

### Check pod status

```bash
oc get pods
oc get routes
```

### View logs

```bash
oc logs deployment/appcontrol-backend --tail=50 -f
oc logs deployment/appcontrol-frontend --tail=50 -f
oc logs deployment/appcontrol-postgres --tail=50 -f
```

### Restart a component

```bash
oc rollout restart deployment/appcontrol-backend
```

### Open a psql shell

```bash
oc exec deployment/appcontrol-postgres -- psql -U appcontrol -d appcontrol
```

### Update to a new version

```bash
IMAGE_TAG=v4.1.0 ./deploy.sh
```

### Delete everything

```bash
./deploy.sh --delete
```

## Sandbox Limitations

| Constraint | Limit |
|------------|-------|
| CPU | 7 cores total |
| Memory | 15 GiB total |
| Storage | 15 GiB total |
| Projects | 2 |
| Idle timeout | Pods sleep after 12h inactivity |
| Lifetime | 30 days, then auto-deleted |

After 12 hours of inactivity, OpenShift scales pods to zero. The first request wakes them up (~30s delay). This is normal behavior for the sandbox.

## Troubleshooting

### Pods stuck in `Pending`

Resource quota exceeded. Check:
```bash
oc describe resourcequota
```

Reduce replicas to 1 in the deployment files (already the default for sandbox).

### Backend CrashLoopBackOff

Usually a database connection issue. Check:
```bash
oc logs deployment/appcontrol-backend
oc get pods   # verify postgres is Running
```

### Route not accessible

Verify routes exist and check TLS:
```bash
oc get routes
curl -I https://<route-host>/health
```

### Image pull errors

If using private GHCR images, create a pull secret:
```bash
oc create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io \
  --docker-username=<github-user> \
  --docker-password=<github-token>
oc secrets link default ghcr-pull --for=pull
```

### PostgreSQL image: why RHEL instead of Alpine?

The OpenShift sandbox uses the `restricted` SecurityContextConstraint (SCC),
which runs containers with a random UID. The Red Hat PostgreSQL image
(`registry.redhat.io/rhel9/postgresql-16`) handles arbitrary UIDs correctly,
while the upstream `postgres:16-alpine` image requires running as the `postgres`
user (UID 999). If you prefer the Alpine image, you can switch it after
verifying it works with your SCC.
