# Upgrade Procedure

This page documents how to upgrade AppControl from one version to the next. It covers the standard rolling upgrade for HA deployments, agent upgrades (which run outside Kubernetes), the air-gap update flow, and rollback procedures.

For HA-specific operational details, see [High Availability](HIGH_AVAILABILITY.md). For initial deployment, see [Production Deployment](PRODUCTION_DEPLOYMENT.md).

## Table of contents

- [Pre-upgrade checklist](#pre-upgrade-checklist)
- [Upgrade order](#upgrade-order)
- [Rolling upgrade for HA](#rolling-upgrade-for-ha)
- [Database migration handling](#database-migration-handling)
- [Agent upgrades](#agent-upgrades)
- [Air-gap upgrade procedure](#air-gap-upgrade-procedure)
- [Rollback](#rollback)
- [Breaking-change handling and deprecation policy](#breaking-change-handling-and-deprecation-policy)
- [Upgrade path table](#upgrade-path-table)

---

## Pre-upgrade checklist

Before any upgrade — major or minor — complete every item on this list.

- [ ] **Read the [CHANGELOG](../CHANGELOG.md).** Identify breaking changes, deprecations, and migration steps.
- [ ] **Take a verified backup.** Run a `pg_dump`, restore it to a sandbox DB, and verify counts via the [restore validation queries](BACKUP_RESTORE.md#restore-validation).
- [ ] **Snapshot the database.** For managed DBs, take a manual snapshot in addition to automated daily backups.
- [ ] **Take a Helm revision snapshot.** `helm get values appcontrol -n appcontrol > pre-upgrade-values.yaml` and `helm history appcontrol -n appcontrol`.
- [ ] **Note your current versions.**
  ```bash
  appctl version
  kubectl exec -n appcontrol deploy/appcontrol-backend -- /app/appcontrol-backend --version
  ```
- [ ] **Verify CI is green on the target version.** Check the project's release page; abort if the target tag is marked "do-not-use".
- [ ] **Identify the maintenance window.** Backend rolling upgrade is zero-downtime; database migrations are not (write pauses are possible).
- [ ] **Disable agent auto-update during the window.** If you use managed agent updates, pause them: `appctl admin agent-update-tasks pause-all`.
- [ ] **Notify stakeholders.** Schedulers may experience brief 503s during backend pod rotation. Notify the operators of any scheduler that hits AppControl (Control-M, AutoSys).
- [ ] **Plan rollback.** Confirm you know which Helm revision to roll back to (`helm history`) and that your backup is restorable.

---

## Upgrade order

Components must be upgraded in this order to avoid protocol mismatches between adjacent layers.

| # | Component | Why |
|--:|-----------|-----|
| 1 | **Database migrations** | Schema must support the new backend before any new backend pod is up |
| 2 | **Backend** | Handles new API surfaces; agents and frontends are backward-compatible to N-1 |
| 3 | **Frontend** | Consumes the new backend API |
| 4 | **Gateway** | Stateless relay, safe to roll at any time after the backend |
| 5 | **Agents** | Depend on backend + gateway being on the new protocol |

Within each component, use a rolling update (Kubernetes `RollingUpdate` strategy by default) so old and new pods coexist briefly. The protocol guarantees N and N-1 interoperability between backend and agents.

!!! warning "Skipping versions"
    AppControl supports **at most one major version** of skip during upgrade (e.g. `v0.3.x → v0.5.x` is allowed; `v0.3.x → v0.6.x` is not). For larger jumps, upgrade through the intermediate major. See the [upgrade path table](#upgrade-path-table).

---

## Rolling upgrade for HA

The default Helm chart uses `RollingUpdate` with `maxUnavailable: 1` on every Deployment. Combined with the `PodDisruptionBudget`, this gives a zero-downtime path.

### Step 1 — update image tags

```bash
# Edit production-values.yaml
backend:
  image:
    tag: "v0.2.0"        # was v0.1.0
frontend:
  image:
    tag: "v0.2.0"
gateway:
  image:
    tag: "v0.2.0"
```

### Step 2 — `helm upgrade`

```bash
helm upgrade appcontrol ./helm/appcontrol \
  --namespace appcontrol \
  --values production-values.yaml \
  --atomic --timeout 10m
```

`--atomic` automatically rolls back if the upgrade fails. `--timeout 10m` is generous; reduce for smaller installs.

### Step 3 — monitor the rollout

```bash
kubectl rollout status -n appcontrol deployment/appcontrol-backend
kubectl rollout status -n appcontrol deployment/appcontrol-gateway
kubectl rollout status -n appcontrol deployment/appcontrol-frontend

# Confirm /ready on every backend pod
for pod in $(kubectl get pods -n appcontrol -l app.kubernetes.io/component=backend -o name); do
  kubectl exec -n appcontrol $pod -- curl -fsS localhost:3000/ready
done
```

### Step 4 — drain semantics

Each replica is replaced one at a time:

1. Kubernetes sends `SIGTERM` to the old pod.
2. Backend handles `SIGTERM` and stops accepting new HTTP connections.
3. Backend drains in-flight requests for up to `SHUTDOWN_TIMEOUT_SECS` (default 30).
4. Kubernetes starts a new pod with the new image.
5. The new pod waits for `/ready` to return 200 before receiving traffic (readiness probe).
6. Old pod terminates; cycle repeats with the next replica.

This means **WebSocket clients (frontend, agents on the gateway) experience a brief disconnect** during their pod's replacement. Frontends reconnect within ~5 s; agents reconnect with exponential backoff (1, 2, 4 s).

---

## Database migration handling

Migrations live in `migrations/` (one file per version, `V0NN__name.sql`). The backend embeds them at compile time and runs `sqlx::migrate` automatically on startup.

### What happens at startup

```
1. Backend pod starts
2. Connects to PostgreSQL
3. Runs sqlx::migrate::Migrator::run(&pool)
4. Migrations execute in transactional batch order
5. If a migration fails:
   - The transaction rolls back
   - Backend logs FATAL and exits non-zero
   - Kubernetes restarts the pod (which fails again)
6. If all migrations succeed:
   - Backend continues startup, becomes /ready
```

### Backward compatibility rule

Migrations must be **backward-compatible** for the duration of the rolling upgrade — that is, the **new** migration must work with the **old** backend code that is still running on the other replicas.

Two patterns:

- **Additive only (safe).** New table, new column with default, new index. Old code ignores them.
- **Removing or renaming (multi-step).** Add the new column → ship a new release that writes both → ship another release that reads from the new column only → ship a third release that drops the old column.

The project follows additive-only for minor releases. Breaking schema changes are deferred to major releases and called out in the [CHANGELOG](../CHANGELOG.md).

### Manual migration (out-of-band)

Run migrations manually only when you want to validate them outside the upgrade window:

```bash
# Connect with the backend container
kubectl run migrate-check --rm -it \
  --image=ghcr.io/fredericcarre/appcontrol-backend:v0.2.0 \
  --env="DATABASE_URL=postgres://appcontrol:..." \
  --command -- /app/appcontrol-backend --migrate-only --dry-run
```

The `--dry-run` flag _(verify in code; was not found at audit time)_ logs the migration plan without executing it.

### If a migration fails

1. Inspect backend logs:
   ```bash
   kubectl logs -n appcontrol -l app.kubernetes.io/component=backend --tail=200 | grep -i migrate
   ```
2. The DB is left in the last good state (transactional rollback). Old backend pods on N-1 continue serving.
3. Either fix the migration (forward-fix) and re-deploy, or roll back the entire upgrade — see [Rollback](#rollback).

---

## Agent upgrades

Agents run outside Kubernetes (on monitored servers) so they need their own upgrade path. Three options are supported. The [Production Deployment guide](PRODUCTION_DEPLOYMENT.md#agent-binary-upgrades) summarizes them; this page goes deeper.

### Option A — managed update (recommended for production)

Backend pushes a signed binary to each agent. Centralized tracking via `agent_update_tasks`.

**Sequence:**

1. **Upload the new binary to the backend.**
   ```bash
   curl -X POST https://appcontrol.example.com/api/v1/admin/agent-binaries \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -F "binary=@appcontrol-agent-linux-amd64-v0.2.0" \
     -F "version=0.2.0" \
     -F "platform=linux-amd64" \
     -F "sha256=$(sha256sum appcontrol-agent-linux-amd64-v0.2.0 | awk '{print $1}')"
   ```
2. **Canary deploy to 2–3 test agents.**
   ```bash
   curl -X POST https://appcontrol.example.com/api/v1/admin/agent-update-tasks \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"agent_ids":["<id1>","<id2>","<id3>"], "version":"0.2.0", "strategy":"immediate"}'
   ```
3. **Verify the canaries.** Check the Agents page for the new version and stable health checks for 30+ min.
4. **Batch-roll to the rest of the fleet.**
   ```bash
   curl -X POST https://appcontrol.example.com/api/v1/admin/agent-update-tasks \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"label_selector":"environment=production", "version":"0.2.0", "strategy":"rolling", "batch_size":10}'
   ```
5. **Monitor.**
   ```bash
   curl https://appcontrol.example.com/api/v1/admin/agent-update-tasks \
     -H "Authorization: Bearer $ADMIN_TOKEN" | jq '.tasks[] | {agent, status, version}'
   ```

The agent verifies the SHA-256 of the downloaded binary against the manifest, swaps atomically (`mv newbin oldbin`), and self-restarts. On failure, the `.old` backup is restored automatically.

### Option B — direct download

Each agent fetches its update directly from the backend's binary endpoint over the existing mTLS WebSocket. Same integrity check and rollback semantics as Option A. Set the agent's `update_channel: pull` in config. _(verify in code; was not found at audit time)_

### Option C — manual replacement

For one-offs or air-gapped hosts not eligible for managed update:

```bash
# Linux
sudo systemctl stop appcontrol-agent
sudo cp appcontrol-agent-linux-amd64-v0.2.0 /usr/local/bin/appcontrol-agent
sudo chmod +x /usr/local/bin/appcontrol-agent
sudo systemctl start appcontrol-agent

# Windows
sc.exe stop AppControlAgent
copy /Y appcontrol-agent-windows-amd64.exe "%ProgramFiles%\AppControl\appcontrol-agent.exe"
sc.exe start AppControlAgent
```

No integrity check, no rollback. Use only when other options are unavailable.

### Verifying the upgrade

After all batches complete:

```sql
SELECT version, count(*) FROM agents WHERE is_active = true GROUP BY version ORDER BY count(*) DESC;
```

Every row should show the new version.

---

## Air-gap upgrade procedure

For environments without internet access. The backend, gateway, and agent containers must be sideloaded into the local registry.

### 1 — Build or download the release bundle

On a connected host, run:

```bash
mkdir release-v0.2.0 && cd release-v0.2.0

# Save container images
for img in backend gateway frontend agent; do
  docker pull ghcr.io/fredericcarre/appcontrol-$img:v0.2.0
  docker save ghcr.io/fredericcarre/appcontrol-$img:v0.2.0 \
    | gzip > appcontrol-$img-v0.2.0.tar.gz
done

# Download agent binaries
for plat in linux-amd64 linux-arm64 darwin-arm64 windows-amd64; do
  gh release download v0.2.0 --pattern "appcontrol-agent-$plat*" \
    --repo fredericcarre/appcontrol
done

# Generate manifest with SHA-256
sha256sum *.tar.gz appcontrol-agent-* > manifest.sha256

# Sign the manifest with your private key
gpg --detach-sign --armor manifest.sha256
```

The result is a self-contained directory:

```
release-v0.2.0/
├── appcontrol-backend-v0.2.0.tar.gz
├── appcontrol-gateway-v0.2.0.tar.gz
├── appcontrol-frontend-v0.2.0.tar.gz
├── appcontrol-agent-v0.2.0.tar.gz
├── appcontrol-agent-linux-amd64
├── appcontrol-agent-linux-arm64
├── appcontrol-agent-darwin-arm64
├── appcontrol-agent-windows-amd64.exe
├── manifest.sha256
└── manifest.sha256.asc
```

### 2 — Transfer to the air-gapped environment

Use whichever sneakernet path your security policy allows (signed USB, dedicated transfer host).

### 3 — Verify on the target side

```bash
# Verify signature
gpg --verify manifest.sha256.asc manifest.sha256

# Verify checksums
sha256sum -c manifest.sha256
# All files: OK
```

If any check fails, abort.

### 4 — Load images into the local registry

```bash
for img in backend gateway frontend agent; do
  gunzip -c appcontrol-$img-v0.2.0.tar.gz | docker load
  docker tag ghcr.io/fredericcarre/appcontrol-$img:v0.2.0 \
             internal.registry.local/appcontrol-$img:v0.2.0
  docker push internal.registry.local/appcontrol-$img:v0.2.0
done
```

### 5 — Upload agent binaries via the admin API

```bash
for plat in linux-amd64 linux-arm64 darwin-arm64 windows-amd64; do
  hash=$(sha256sum appcontrol-agent-$plat | awk '{print $1}')
  curl -X POST https://appcontrol.internal/api/v1/admin/agent-binaries \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    -F "binary=@appcontrol-agent-$plat" \
    -F "version=0.2.0" -F "platform=$plat" -F "sha256=$hash"
done
```

### 6 — Run a canary

Pick 2–3 agents in a non-critical zone. Push the update. Verify health checks resume and version is reported as 0.2.0. Wait 30+ min, ideally 24 h, before rolling to the rest.

### 7 — Roll the rest of the fleet

Same procedure as the managed update (Option A above), but `binary_url` points at the air-gapped backend, which serves the file from internal storage.

---

## Rollback

### Backend, frontend, gateway

```bash
# Find the previous Helm revision
helm history appcontrol -n appcontrol

# Roll back
helm rollback appcontrol -n appcontrol <revision>

# Verify
kubectl rollout status -n appcontrol deployment/appcontrol-backend
```

`helm rollback` reverts image tags and pod templates. **It does not revert database migrations.**

### Database

Rollback of a forward migration is **not** automatic. Forward migrations are designed to be additive (see above). If a major release introduced a destructive migration, the project provides a paired down-migration in the same release (`V0NN_down.sql`).

To run a down-migration:

```bash
# Connect to the DB and run the explicit down-migration
psql $DATABASE_URL < migrations/V040_down.sql
```

In practice, AppControl's migrations are additive and a Helm rollback of the backend image is sufficient. If you cannot continue forward and a destructive migration has run, your last resort is restoring from backup (see [Backup & Restore](BACKUP_RESTORE.md)).

### Agents

Managed updates store the previous binary as `.appcontrol-agent.old` next to the live binary. If the new agent fails its post-update health check, the `.old` binary is restored automatically and the agent self-restarts. To manually trigger a rollback:

```bash
sudo systemctl stop appcontrol-agent
sudo mv /usr/local/bin/.appcontrol-agent.old /usr/local/bin/appcontrol-agent
sudo systemctl start appcontrol-agent
```

For Windows:

```powershell
sc.exe stop AppControlAgent
move "%ProgramFiles%\AppControl\appcontrol-agent.exe.old" "%ProgramFiles%\AppControl\appcontrol-agent.exe"
sc.exe start AppControlAgent
```

### Re-test after rollback

Always run the post-upgrade verification suite after rollback:

```bash
appctl status --all
kubectl exec -n appcontrol deploy/appcontrol-backend -- curl -fsS localhost:3000/ready
```

---

## Breaking-change handling and deprecation policy

AppControl follows [Semantic Versioning](https://semver.org/) (semver):

| Change type | Version bump | Examples |
|-------------|--------------|----------|
| Patch | `0.1.0 → 0.1.1` | Bug fixes, doc fixes, performance |
| Minor | `0.1.0 → 0.2.0` | New endpoints, new fields, new env vars |
| Major | `0.x → 1.0` | Removed endpoints, schema-breaking migrations, config-format changes |

### Deprecation policy

- A feature is **announced deprecated** in a minor release. The CHANGELOG lists it under `### Deprecated`.
- The deprecated feature continues to work for **at least one major release cycle**.
- The feature is removed in the next major release (called out under `### Removed`).
- Where possible, a server warning header (`Sunset: ...`, `Warning: ...`) flags deprecated API calls.

### How to read the CHANGELOG

Open `CHANGELOG.md`. For the target version:

- `### Added` — new functionality. Usually safe.
- `### Changed` — behavior change. Read carefully; may require operator action.
- `### Fixed` — bug fix.
- `### Deprecated` — feature still works but will be removed.
- `### Removed` — feature is gone. **Required** action for callers who rely on it.
- `### Security` — security fix. Upgrade ASAP.

For any non-additive change, the entry includes "Migration steps" with explicit commands.

---

## Upgrade path table

This table lists the supported upgrade paths between recent major and minor versions. "Direct" means the rolling-upgrade procedure above works without intermediate steps. "Multi-step" means you must first upgrade to an intermediate version.

| From | To | Path | Notes |
|------|----|------|-------|
| 0.1.x | 0.2.x | Direct | Migrations are additive |
| 0.1.x | 0.3.x | Direct | Phase 9 (sharing/API keys) is additive |
| 0.1.x | 0.4.x | Multi-step: 0.1 → 0.3 → 0.4 | Phase 10 removes the Redis dependency; if you ran with Redis previously, run 0.3.x first to migrate state to PostgreSQL |
| 0.1.x | 0.5.x | Multi-step: 0.1 → 0.3 → 0.5 | Same as 0.4 plus partition reorganization |
| 0.2.x | 0.3.x | Direct | Additive |
| 0.2.x | 0.4.x | Direct | Phase 10 migration handles Redis removal cleanly from 0.2 (no Redis was used) |
| 0.3.x | 0.4.x | Direct | The `redis` crate is removed from dependencies; if `REDIS_URL` is set in your env, the backend logs a warning and ignores it |
| 0.4.x | 0.5.x | Direct | Additive |
| 0.5.x | 0.6.x | Direct | Additive (planned features around clustering and binding profiles) |

For exact migration entry points, consult the [CHANGELOG](../CHANGELOG.md) "Upgrading from X.Y" section in each release.

### Verifying the path

Before any upgrade, validate the path on a staging cluster:

```bash
# Sandbox upgrade
helm upgrade appcontrol-staging ./helm/appcontrol \
  --namespace appcontrol-staging \
  --values staging-values.yaml \
  --set backend.image.tag=v0.4.0

# Wait, then run integration smoke tests
./scripts/smoke-test.sh https://appcontrol.staging
```

If smoke tests pass, repeat in production during your maintenance window.

---

## Reference

- [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md) — initial Helm install
- [HIGH_AVAILABILITY.md](HIGH_AVAILABILITY.md) — HA topology and failure modes
- [BACKUP_RESTORE.md](BACKUP_RESTORE.md) — backups consumed during rollback
- [CHANGELOG](../CHANGELOG.md) — per-version notes
- [AGENT_INSTALLATION.md §12](AGENT_INSTALLATION.md) — full agent upgrade detail
