# Production Deployment Guide

This guide covers deploying AppControl in a production Kubernetes environment.

## Prerequisites

| Component | Version | Notes |
|-----------|---------|-------|
| Kubernetes | 1.26+ | EKS, GKE, AKS, or OpenShift 4.12+ |
| Helm | 3.12+ | |
| PostgreSQL | 16+ | Managed (RDS, CloudSQL, Azure DB) recommended |
| cert-manager | 1.12+ | For TLS certificate management |
| kubectl | 1.26+ | Matching cluster version |

## Architecture Overview

```
                    ┌──────────────┐
                    │  Ingress /   │
                    │  Load Balancer│
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
         ┌────▼───┐  ┌────▼───┐  ┌────▼────┐
         │Frontend│  │Backend │  │ Gateway  │
         │(nginx) │  │(Axum)  │  │ (Axum)  │
         │ x2     │  │ x2     │  │  x2     │
         └────────┘  └──┬──┬──┘  └────┬────┘
                        │  │          │
              ┌─────────┘  └────┐     │  Agents connect
              │                 │     │  via mTLS
         ┌────▼─────┐               │
         │PostgreSQL │               ▼
         │  (HA)     │               🖥 Agents
         │ REQUIRED  │
         └───────────┘

Note: Only the Backend connects to PostgreSQL.
Gateways and Agents have no database dependency.
```

## Step 1: Namespace and Secrets

```bash
# Create namespace
kubectl create namespace appcontrol

# Create database secret (use your managed DB credentials)
kubectl create secret generic appcontrol-db \
  --namespace appcontrol \
  --from-literal=database-url="postgres://appcontrol:PASSWORD@your-rds-host:5432/appcontrol?sslmode=require"

# Create JWT signing secret
kubectl create secret generic appcontrol-jwt \
  --namespace appcontrol \
  --from-literal=jwt-secret="$(openssl rand -base64 64)"

```

## Step 2: TLS Certificates

### Option A: cert-manager (recommended)

```yaml
# issuer.yaml
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: appcontrol-ca
spec:
  selfSigned: {}
---
# For agent mTLS: create a CA for signing agent certificates
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: appcontrol-ca-cert
  namespace: appcontrol
spec:
  isCA: true
  commonName: appcontrol-ca
  secretName: appcontrol-ca-secret
  issuerRef:
    name: appcontrol-ca
    kind: ClusterIssuer
---
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: appcontrol-agent-issuer
  namespace: appcontrol
spec:
  ca:
    secretName: appcontrol-ca-secret
---
# Gateway server certificate
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: appcontrol-gateway-cert
  namespace: appcontrol
spec:
  secretName: appcontrol-gateway-tls
  issuerRef:
    name: appcontrol-agent-issuer
  commonName: appcontrol-gateway
  dnsNames:
    - appcontrol-gateway
    - appcontrol-gateway.appcontrol.svc.cluster.local
    - gateway.your-domain.com
```

### Option B: Manual certificates

```bash
# Generate CA
openssl genrsa -out ca.key 4096
openssl req -new -x509 -key ca.key -out ca.crt -days 3650 \
  -subj "/CN=AppControl CA"

# Generate gateway certificate
openssl genrsa -out gateway.key 2048
openssl req -new -key gateway.key -out gateway.csr \
  -subj "/CN=appcontrol-gateway"
openssl x509 -req -in gateway.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out gateway.crt -days 365

# Generate agent certificate (one per agent)
openssl genrsa -out agent-01.key 2048
openssl req -new -key agent-01.key -out agent-01.csr \
  -subj "/CN=agent-01"
openssl x509 -req -in agent-01.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out agent-01.crt -days 365

# Create Kubernetes secrets
kubectl create secret generic appcontrol-gateway-tls \
  --namespace appcontrol \
  --from-file=tls.crt=gateway.crt \
  --from-file=tls.key=gateway.key \
  --from-file=ca.crt=ca.crt
```

## Step 3: Database Setup

### Managed PostgreSQL (recommended)

Create a PostgreSQL 16 instance in your cloud provider:

- **AWS RDS**: `db.r6g.large`, Multi-AZ, automated backups
- **GCP CloudSQL**: `db-custom-2-7680`, HA with failover replica
- **Azure Database**: `GP_Gen5_2`, Zone-redundant HA

Key settings:

```
max_connections = 200
shared_buffers = 2GB
effective_cache_size = 6GB
work_mem = 16MB
maintenance_work_mem = 512MB
wal_level = replica          # For PITR
archive_mode = on
archive_command = 'your-wal-archive-command'
```

### In-cluster PostgreSQL (dev/staging only)

The Helm chart includes a PostgreSQL StatefulSet with PVC. Set `postgresql.enabled: true` in values.

## Step 4: Helm Values for Production

Create `production-values.yaml`:

```yaml
global:
  imageRegistry: "your-registry.example.com/"
  imagePullSecrets:
    - name: regcred

backend:
  replicaCount: 3
  image:
    tag: "v0.1.0"
  resources:
    requests:
      cpu: "1"
      memory: 1Gi
    limits:
      cpu: "2"
      memory: 2Gi
  env:
    RUST_LOG: "info"
    CORS_ORIGINS: "https://appcontrol.your-domain.com"
  database:
    existingSecret: "appcontrol-db"
    secretKey: "database-url"
  jwt:
    existingSecret: "appcontrol-jwt"
    secretKey: "jwt-secret"

frontend:
  replicaCount: 2
  image:
    tag: "v0.1.0"

gateway:
  replicaCount: 2
  image:
    tag: "v0.1.0"
  resources:
    requests:
      cpu: 500m
      memory: 256Mi
    limits:
      cpu: "1"
      memory: 512Mi

# Disable in-cluster databases for production
postgresql:
  enabled: false

podDisruptionBudget:
  enabled: true
  backend:
    minAvailable: 2
  gateway:
    minAvailable: 1
  frontend:
    minAvailable: 1

ingress:
  enabled: true
  className: "nginx"
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "3600"
  hosts:
    - host: appcontrol.your-domain.com
      paths:
        - path: /api
          pathType: Prefix
        - path: /ws
          pathType: Prefix
        - path: /
          pathType: Prefix
  tls:
    - secretName: appcontrol-tls
      hosts:
        - appcontrol.your-domain.com

networkPolicy:
  enabled: true
```

## Step 6: Deploy

```bash
# Add the Helm chart
helm install appcontrol ./helm/appcontrol \
  --namespace appcontrol \
  --values production-values.yaml

# Verify deployment
kubectl get pods -n appcontrol
kubectl get pdb -n appcontrol

# Check backend health
kubectl exec -n appcontrol deploy/appcontrol-backend -- curl -s localhost:3000/health
kubectl exec -n appcontrol deploy/appcontrol-backend -- curl -s localhost:3000/ready
```

## Step 7: Monitoring Setup

### Prometheus

The backend exposes metrics at `/metrics` (Prometheus exposition format):

```yaml
# prometheus-scrape-config.yaml
scrape_configs:
  - job_name: appcontrol-backend
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names: [appcontrol]
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_label_app_kubernetes_io_component]
        regex: backend
        action: keep
```

Key metrics:
- `http_requests_total` — Request count by method and status
- `http_request_duration_seconds` — Request latency histogram
- `ws_connections_active` — Active WebSocket connections
- `agents_connected` — Number of connected agents
- `state_transitions_total` — FSM state changes
- `db_pool_connections` — Database pool utilization

### Grafana Dashboard

Create a Grafana dashboard with panels for:
- Request rate and error rate (from `http_requests_total`)
- P50/P95/P99 latency (from `http_request_duration_seconds`)
- Agent connection count over time
- Database pool saturation
- State transition rate

## Backup Strategy

### PostgreSQL PITR (Point-in-Time Recovery)

For managed databases, enable automated backups:

- **AWS RDS**: Automated backups with 35-day retention, enable PITR
- **GCP CloudSQL**: Automated daily backups, enable PITR with WAL
- **Azure DB**: Geo-redundant backup with 35-day retention

For in-cluster PostgreSQL:

```bash
# Manual backup
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  pg_dump -U appcontrol -Fc appcontrol > backup_$(date +%Y%m%d).dump

# Restore
kubectl exec -i -n appcontrol appcontrol-postgresql-0 -- \
  pg_restore -U appcontrol -d appcontrol --clean < backup_20260223.dump
```

### Configuration Snapshots

AppControl automatically stores config snapshots (before/after JSONB) in `config_versions`. No manual backup needed for configuration.

### Append-Only Event Tables

Tables `action_log`, `state_transitions`, `check_events`, `switchover_log` are append-only. They are automatically partitioned by month. Old partitions are dropped by the data retention policy (configurable via `RETENTION_CHECK_EVENTS_DAYS`).

## Upgrade Procedure

### Recommended upgrade order

Upgrade components in this order to avoid protocol mismatches:

1. **Backend** — handles database migrations and API changes
2. **Frontend** — depends on updated API
3. **Gateways** — stateless relays, safe to roll at any time
4. **Agents** — depend on gateway + backend being ready

### Backend, Frontend & Gateway (Helm rolling update)

```bash
# 1. Update image tags in production-values.yaml
# 2. Helm upgrade (rolling update, zero downtime thanks to PDB)
helm upgrade appcontrol ./helm/appcontrol \
  --namespace appcontrol \
  --values production-values.yaml

# 3. Monitor rollout
kubectl rollout status -n appcontrol deployment/appcontrol-backend
kubectl rollout status -n appcontrol deployment/appcontrol-gateway
kubectl rollout status -n appcontrol deployment/appcontrol-frontend

# 4. Verify health
kubectl exec -n appcontrol deploy/appcontrol-backend -- curl -s localhost:3000/ready
```

Gateway upgrades are **zero-downtime** thanks to:

- **PodDisruptionBudget** (`minAvailable: 1`) prevents all gateway pods from
  terminating simultaneously.
- **Agent-driven failover**: agents configured with multiple `gateway.urls`
  automatically reconnect to the next available gateway when a pod terminates
  during a rolling update.

### Agent binary upgrades

Agents run outside Kubernetes (on monitored servers) and require a separate
upgrade path. Three options are available, detailed in the
[Agent Installation Guide — Upgrading Agents](AGENT_INSTALLATION.md#12-upgrading-agents):

| Method | Best for | Integrity check | Rollback | Centralized tracking |
|--------|----------|:-:|:-:|:-:|
| Managed update (API) | Air-gapped or production | Yes (SHA-256) | Automatic | Yes (`agent_update_tasks`) |
| Direct download | Connected agents | Yes (SHA-256) | Automatic | Yes |
| Manual replacement | Emergency or dev | No | Manual | No |

**Recommended approach for production:**

1. Upload the new agent binary to the backend via API.
2. Push the update to 2–3 test agents. Verify health checks resume and the
   correct version appears in the Agent Management UI.
3. Batch-upgrade remaining agents using the batch update API.
4. Monitor progress via `GET /api/v1/admin/agent-update-tasks`.

### Gateway High Availability

Gateways are **stateless WebSocket relays** with no database dependency. High
availability is achieved through agent-driven failover, not server-side
clustering.

**Agent-side failover configuration** (in `agent.yaml`):

```yaml
gateway:
  urls:
    - "wss://gw-primary.prod:4443/ws"
    - "wss://gw-secondary.prod:4443/ws"
  failover_strategy: "ordered"   # or "round-robin"
  primary_retry_secs: 300        # Retry primary every 5 minutes
```

**Gateway maintenance workflow:**

1. Suspend the gateway in the UI or API (`is_active = false`).
2. Connected agents detect the disconnection and failover to the next gateway.
3. Perform maintenance or upgrade on the suspended gateway.
4. Re-enable the gateway (`is_active = true`).
5. Agents with `failover_strategy: "ordered"` will return to the primary
   gateway within `primary_retry_secs`.

## Rollback

### Backend, Frontend & Gateway

```bash
# Helm rollback to previous revision
helm rollback appcontrol -n appcontrol

# Verify
kubectl rollout status -n appcontrol deployment/appcontrol-backend
```

### Agents

If a managed update fails, the agent automatically restores the `.old` binary
backup. For manual rollbacks:

```bash
# The previous binary is saved as .appcontrol-agent.old
sudo systemctl stop appcontrol-agent
sudo mv /usr/local/bin/.appcontrol-agent.old /usr/local/bin/appcontrol-agent
sudo systemctl start appcontrol-agent
```
