# Production Deployment Guide

This guide covers deploying AppControl in a production Kubernetes environment.

## Prerequisites

| Component | Version | Notes |
|-----------|---------|-------|
| Kubernetes | 1.26+ | EKS, GKE, AKS, or OpenShift 4.12+ |
| Helm | 3.12+ | |
| PostgreSQL | 16+ | Managed (RDS, CloudSQL, Azure DB) recommended |
| Redis | 7+ | Managed with failover recommended |
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
         ┌────▼───┐       ┌────▼──┐  │
         │PostgreSQL│      │ Redis │  ▼
         │  (HA)   │      │ (HA)  │  🖥 Agents
         └─────────┘      └───────┘
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

# Create Redis auth secret (if using auth)
kubectl create secret generic appcontrol-redis \
  --namespace appcontrol \
  --from-literal=redis-password="$(openssl rand -base64 32)"
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

## Step 4: Redis Setup

### Managed Redis (recommended)

- **AWS ElastiCache**: Redis 7, `cache.r6g.large`, Multi-AZ with automatic failover
- **GCP Memorystore**: Redis 7, Standard tier (HA)
- **Azure Cache for Redis**: Premium P1 with zone redundancy

### In-cluster Redis (dev/staging only)

The Helm chart includes a Redis StatefulSet with AOF persistence. Set `redis.enabled: true` in values.

## Step 5: Helm Values for Production

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
  redis:
    url: "redis://your-elasticache-host:6379"
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

redis:
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

Import the dashboard from `docs/grafana-dashboard.json` or create panels for:
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

```bash
# 1. Update image tags in values
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

## Rollback

```bash
# Helm rollback to previous revision
helm rollback appcontrol -n appcontrol

# Verify
kubectl rollout status -n appcontrol deployment/appcontrol-backend
```
