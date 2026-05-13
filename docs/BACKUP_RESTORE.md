# Backup & Restore

This page documents how to back up and restore the AppControl backend state. AppControl persists everything required for resilience and audit in PostgreSQL (or SQLite for small deployments). Gateways and agents are stateless from a backup perspective — they hold mTLS certs (re-issuable) and an ephemeral local buffer (≤ 100 MB sled).

For DR scenarios that consume these backups, see [Disaster Recovery](DISASTER_RECOVERY.md).

## Table of contents

- [What to back up](#what-to-back-up)
- [PostgreSQL](#postgresql)
  - [pg_dump — full backup](#pg_dump--full-backup)
  - [WAL archiving for PITR](#wal-archiving-for-pitr)
  - [Partitioned `check_events`](#partitioned-check_events)
- [SQLite](#sqlite)
- [What can be regenerated vs preserved](#what-can-be-regenerated-vs-preserved)
- [Restore validation](#restore-validation)
- [Migration from SQLite to PostgreSQL](#migration-from-sqlite-to-postgresql)
- [Keys & secrets](#keys--secrets)
- [Sample crontab and retention policy](#sample-crontab-and-retention-policy)

---

## What to back up

| Asset | Storage | Recovery if lost |
|-------|---------|------------------|
| PostgreSQL data directory | `/var/lib/postgresql/data/` (or managed service) | From backup |
| Append-only audit (`action_log`, `state_transitions`, `check_events`, `switchover_log`, `config_versions`) | PostgreSQL tables | From backup — **no other source** |
| CA private key | `pki_authorities` table (encrypted) | From backup — losing it forces re-enrollment of every agent |
| `JWT_SECRET` | Kubernetes secret `appcontrol-jwt` | Can be regenerated (invalidates all sessions) |
| `DATABASE_URL` | Kubernetes secret `appcontrol-db` | Known config |
| OIDC / SAML client secrets | env vars or Kubernetes secrets | Re-issue from your IdP |
| Gateway certs | Kubernetes secret `appcontrol-gateway-tls` | Re-issue with `appctl pki issue-gateway` |
| Agent certs | `/etc/appcontrol/tls/` on each host | Re-enroll if CA is intact; restore from full host backup otherwise |
| Helm values, config-management state | git / SCM | From git history |

---

## PostgreSQL

### pg_dump — full backup

The standard tool for logical backups. Produces a portable `.dump` file usable across minor versions.

```bash
# Backend uses no extensions beyond gen_random_uuid (pgcrypto); a logical dump is sufficient
pg_dump \
  --host=db.internal \
  --username=appcontrol \
  --format=custom \
  --compress=9 \
  --file=appcontrol-$(date +%Y%m%d-%H%M).dump \
  appcontrol
```

Restore:

```bash
pg_restore \
  --host=db-new.internal \
  --username=appcontrol \
  --clean --if-exists \
  --no-acl --no-owner \
  --jobs=4 \
  --dbname=appcontrol \
  appcontrol-20260513-0300.dump
```

!!! example "Verifying the dump"
    A backup that cannot be restored is not a backup. Always:

    1. Restore to a sandbox PostgreSQL instance after every cron run.
    2. Run the [validation queries](#restore-validation) below.
    3. Alert if any check fails.

#### Incremental backups

PostgreSQL has no native "incremental" logical dump. For incremental, use either:

- **`pg_dump --table=` per-table** scoped to mutable tables (small) plus periodic full dumps of immutable tables.
- **WAL archiving** (recommended; see below).

### WAL archiving for PITR

Point-in-time recovery requires continuous WAL archiving plus periodic base backups. Recommended setup:

```ini
# postgresql.conf
wal_level = replica
archive_mode = on
archive_command = 'aws s3 cp %p s3://appcontrol-wal/%f --no-progress'
archive_timeout = 60          # seconds — force a switch every 60s for tight RPO
max_wal_senders = 3
```

Take periodic base backups (e.g. daily 03:00):

```bash
pg_basebackup \
  --host=db.internal \
  --username=replicator \
  --pgdata=/backup/base-$(date +%Y%m%d) \
  --format=tar --gzip \
  --wal-method=stream \
  --checkpoint=fast
```

Restore to a target time:

```bash
# 1. Extract base backup
tar -xzf /backup/base-20260513.tar.gz -C /var/lib/postgresql/data

# 2. Tell PostgreSQL where to fetch WAL and when to stop
cat >> /var/lib/postgresql/data/postgresql.auto.conf <<EOF
restore_command = 'aws s3 cp s3://appcontrol-wal/%f %p'
recovery_target_time = '2026-05-13 10:30:00 UTC'
EOF

touch /var/lib/postgresql/data/recovery.signal
systemctl start postgresql
```

**RPO with archive_timeout=60s:** typically 1–2 min in the worst case (60 s of unarchived WAL + sync lag to S3).

### Partitioned `check_events`

`check_events` is partitioned by month (migration `V005`). Each monthly partition is an independent PostgreSQL table:

```sql
SELECT inhrelid::regclass AS partition,
       pg_size_pretty(pg_total_relation_size(inhrelid)) AS size
FROM pg_inherits WHERE inhparent = 'check_events'::regclass
ORDER BY partition;
-- check_events_y2026m01    8 GB
-- check_events_y2026m02    9 GB
-- check_events_y2026m03    11 GB
```

#### Per-partition dump

For very large `check_events`, dump each partition separately to limit dump time:

```bash
for p in $(psql -At $DATABASE_URL -c \
    "SELECT inhrelid::regclass FROM pg_inherits WHERE inhparent = 'check_events'::regclass"); do
  pg_dump --format=custom --compress=9 \
    --table="$p" --file="ce-${p}.dump" $DATABASE_URL
done
```

Restore:

```bash
for f in ce-*.dump; do
  pg_restore --dbname=$DATABASE_URL_NEW "$f"
done
```

This is dramatically faster than restoring the full DB because partition restores parallelize trivially.

#### Skipping cold partitions

You can choose to not back up partitions older than your retention requirement (e.g. > 6 months):

```bash
# Drop locally, no backup, before they're rotated by retention
psql $DATABASE_URL -c "DROP TABLE check_events_y2025m11;"
```

This is a regulatory decision — DORA Article 16 expects incident records for 5+ years; check policy before dropping.

---

## SQLite

AppControl supports SQLite for small / air-gapped / single-host deployments (see [SQLite Implementation](SQLITE_IMPLEMENTATION.md)).

### Live backup with `.backup`

SQLite's `.backup` command produces a transactionally consistent copy of the live DB without locking:

```bash
# From the backend host
sqlite3 /var/lib/appcontrol/appcontrol.db ".backup '/backup/appcontrol-$(date +%Y%m%d-%H%M).db'"
```

This uses the SQLite online backup API: it copies pages while the backend continues to write.

### rsync (cold backup)

For an *idle* database (backend stopped), `rsync` is fastest:

```bash
systemctl stop appcontrol-backend
rsync -a /var/lib/appcontrol/appcontrol.db /backup/appcontrol.db
systemctl start appcontrol-backend
```

This causes a service outage; prefer `.backup` for live systems.

### Verify integrity

```bash
sqlite3 /backup/appcontrol.db "PRAGMA integrity_check;"
# Should return: ok
```

---

## What can be regenerated vs preserved

The system distinguishes **append-only** from **mutable** tables. Append-only tables are the regulatory record; losing them is a compliance breach.

### APPEND-ONLY — must be preserved

| Table | Purpose | DORA mapping |
|-------|---------|--------------|
| `action_log` | Every user action, written before execute | Art. 16 (incident records) |
| `state_transitions` | Every FSM state change | Art. 16 |
| `check_events` | Every health/integrity/infra check (partitioned) | Art. 8(2), Art. 11 |
| `switchover_log` | DR switchover phase-by-phase records | Art. 11, Art. 25 |
| `config_versions` | Every config change with before/after JSONB | Art. 8 |
| `enrollment_audit` _(if present)_ | Token uses for agent/gateway enrollment | Art. 16 |
| `break_glass_sessions` | Emergency access sessions | Art. 16 |

### REGENERABLE — backup nice-to-have

| Table | Regenerated by |
|-------|----------------|
| `components.current_state` | The next health-check cycle |
| `fsm_cache` / `cluster_member_state` | Agent reconnect + first check |
| `agent_metrics` | Resumed on agent reconnect |
| `revoked_tokens` | Tokens auto-expire; entries are short-lived |
| `notification_queue` | Notifications re-delivered if subscribed |

### CONFIG — back up via Helm/secrets

- `organizations`, `users`, `teams`, `team_members`, `app_permissions_users/teams`, `applications`, `components`, `dependencies`, `sites`, `hostings`, `binding_profiles`, `agents`, `gateways`, `api_keys`, `enrollment_tokens`.

These tables hold the operational configuration. They are not append-only — they evolve over time — but they are critical to restore exactly. A full PostgreSQL dump covers them.

---

## Restore validation

Run this checklist after every restore (DR drill or production).

### 1. Row counts

```sql
SELECT
  (SELECT count(*) FROM organizations)         AS organizations,
  (SELECT count(*) FROM users)                 AS users,
  (SELECT count(*) FROM applications)          AS applications,
  (SELECT count(*) FROM components)            AS components,
  (SELECT count(*) FROM dependencies)          AS dependencies,
  (SELECT count(*) FROM agents)                AS agents,
  (SELECT count(*) FROM action_log)            AS action_log,
  (SELECT count(*) FROM state_transitions)     AS state_transitions,
  (SELECT count(*) FROM switchover_log)        AS switchover_log;
```

Compare to the last known-good counts (snapshot them before the disaster).

### 2. Last action_log entry

```sql
SELECT max(created_at) AS last_action FROM action_log;
-- Should be close to the recovery_target_time (PITR) or just before the backup time
```

### 3. Foreign-key integrity

```sql
-- Components without an application (should be 0)
SELECT count(*) FROM components c
LEFT JOIN applications a ON a.id = c.application_id
WHERE a.id IS NULL;

-- action_log without a user (should be 0)
SELECT count(*) FROM action_log al
LEFT JOIN users u ON u.id = al.user_id
WHERE u.id IS NULL AND al.user_id IS NOT NULL;

-- check_events partitions all present
SELECT generate_series('2026-01-01'::date, current_date, '1 month'::interval) AS month
EXCEPT
SELECT date_trunc('month', tableoid::regclass::text::date)
FROM check_events LIMIT 1;
```

### 4. Backend `/ready`

```bash
kubectl exec -n appcontrol deploy/appcontrol-backend -- curl -s localhost:3000/ready
# {"status":"ready"}
```

### 5. Agent reconnection

Within the heartbeat timeout window (default 180 s), expect all previously-active agents to reconnect:

```sql
SELECT
  count(*) FILTER (WHERE last_heartbeat_at > now() - interval '5 minutes') AS connected,
  count(*) FILTER (WHERE last_heartbeat_at <= now() - interval '5 minutes') AS disconnected
FROM agents WHERE is_active = true;
```

`disconnected` should approach 0 within 5 minutes.

### 6. Application state

```sql
-- Components in UNKNOWN should resolve within one check_interval
SELECT current_state, count(*) FROM components GROUP BY current_state;
```

---

## Migration from SQLite to PostgreSQL

If you outgrew SQLite, migrate to PostgreSQL with the following procedure. The DB schema is bytewise identical apart from UUID encoding (TEXT on SQLite, native UUID on PostgreSQL) and JSON column types (`TEXT` vs `JSONB`).

### Step 1 — provision PostgreSQL

```bash
docker run -d --name pg-target \
  -e POSTGRES_DB=appcontrol \
  -e POSTGRES_USER=appcontrol \
  -e POSTGRES_PASSWORD=appcontrol \
  -p 5432:5432 postgres:16-alpine
```

### Step 2 — run migrations on the empty target

```bash
DATABASE_URL=postgres://appcontrol:appcontrol@localhost:5432/appcontrol \
  appcontrol-backend --migrate-only
```

### Step 3 — export from SQLite

For each table in dependency order (see [migrations/CLAUDE.md](https://github.com/fredericcarre/appcontrol/blob/main/migrations/CLAUDE.md)):

```bash
sqlite3 /var/lib/appcontrol/appcontrol.db <<EOF
.mode csv
.headers on
.output /tmp/organizations.csv
SELECT * FROM organizations;
.output /tmp/users.csv
SELECT * FROM users;
-- repeat for each table
EOF
```

### Step 4 — transform and import

UUIDs in SQLite are stored as TEXT in canonical form (`550e8400-...`). PostgreSQL accepts the same form when inserted into `UUID` columns.

JSON columns in SQLite are TEXT; PostgreSQL wants `::jsonb`. The migration helper handles the cast on import:

```bash
psql $DATABASE_URL <<'EOF'
\copy organizations FROM '/tmp/organizations.csv' WITH (FORMAT csv, HEADER true);
\copy users FROM '/tmp/users.csv' WITH (FORMAT csv, HEADER true);
-- ...

-- Re-cast JSON columns if needed
UPDATE organizations SET settings = settings::text::jsonb;
EOF
```

### Step 5 — switch the backend

```bash
# Update the secret
kubectl create secret generic appcontrol-db --namespace appcontrol \
  --from-literal=database-url="postgres://appcontrol:appcontrol@pg.internal:5432/appcontrol?sslmode=require" \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart the backend
kubectl rollout restart -n appcontrol deployment/appcontrol-backend
```

### Step 6 — run validation queries

Use the [restore validation queries](#restore-validation) above.

!!! warning "Migration is one-way"
    There is no automated PostgreSQL → SQLite path. SQLite is intended for small installs (< 50 agents); once you migrate, plan to stay on PostgreSQL.

---

## Keys & secrets

### CA private key

The per-organization CA is stored encrypted in the `pki_authorities` table. If you lose this row:

- **Every agent must be re-enrolled** with a new CA — there is no other way.
- Plan for this by including `pki_authorities` in your DB backup (it is part of the `pg_dump`).

For high security, consider an **external CA**: provision the CA in HashiCorp Vault PKI or AWS KMS, and configure the backend to delegate signing. _(verify in code; was not found at audit time)_

### `JWT_SECRET`

Stored in the Kubernetes secret `appcontrol-jwt`. Back it up the same way you back up other Kubernetes secrets (Sealed Secrets, HashiCorp Vault, AWS Secrets Manager). Losing it forces every user to re-authenticate.

```bash
kubectl get secret -n appcontrol appcontrol-jwt -o yaml > appcontrol-jwt.yaml.enc
# Store encrypted in your secret store
```

### Agent client certificates

Each agent host has its own client cert at `/etc/appcontrol/tls/agent.crt`. If the host is wiped:

- If the CA is intact, re-enroll: `appcontrol-agent --enroll …`.
- If the CA is gone, see CA recovery above.

### Gateway server certificate

Stored as Kubernetes secret `appcontrol-gateway-tls`. Re-issue at any time via `appctl pki issue-gateway`.

### KMS recommendation

For production, store the CA private key in a KMS (AWS KMS, GCP Cloud KMS, Azure Key Vault) or HSM and have the backend call the KMS for signing. This:

- Prevents an admin from exfiltrating the CA from the backend DB.
- Satisfies DORA Article 9 (cryptographic key management).
- Allows hardware-backed key rotation.

Configuration: set `PKI_BACKEND=vault` and provide `VAULT_ADDR`, `VAULT_TOKEN` (or AppRole). _(verify in code; was not found at audit time)_

---

## Sample crontab and retention policy

### Recommended cadence

| Cadence | What | Retention |
|---------|------|-----------|
| Every 1 min | WAL archive segment to S3 | 30 days |
| Daily 03:00 | Full `pg_dump` of all DBs | 30 daily files |
| Weekly Sun 04:00 | Tar+gzip last 7 daily dumps to cold storage | 12 weekly archives |
| Monthly 1st 05:00 | Promote last weekly to year-tier cold storage | 12 monthly + 7 yearly archives |
| Daily 06:00 | `pg_basebackup` for fast PITR base | 7 base backups |
| Daily 23:00 | Sandbox-restore the latest daily and run validation | n/a |

### crontab example

```cron
# m h dom mon dow command
0  3  *  *  *  /usr/local/bin/appcontrol-backup-daily.sh
0  4  *  *  0  /usr/local/bin/appcontrol-backup-weekly.sh
0  5  1  *  *  /usr/local/bin/appcontrol-backup-monthly.sh
0  6  *  *  *  /usr/local/bin/appcontrol-basebackup.sh
0 23  *  *  *  /usr/local/bin/appcontrol-restore-verify.sh
```

### `appcontrol-backup-daily.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

DEST="s3://appcontrol-backups/daily/$(date +%Y/%m/%d)"
DUMP="/tmp/appcontrol-$(date +%Y%m%d-%H%M).dump"

pg_dump --host=db.internal --username=appcontrol \
        --format=custom --compress=9 \
        --file="$DUMP" appcontrol

# Server-side encryption for at-rest protection
aws s3 cp "$DUMP" "$DEST/" \
    --sse aws:kms \
    --sse-kms-key-id "$KMS_KEY_ID"

rm -f "$DUMP"

# Retention: delete > 30 days
aws s3 ls s3://appcontrol-backups/daily/ --recursive |
  awk '$1 < "'$(date -d '30 days ago' +%Y-%m-%d)'"' |
  awk '{print $4}' |
  xargs -r -I {} aws s3 rm "s3://appcontrol-backups/{}"
```

### `appcontrol-restore-verify.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Find the most recent daily backup
LATEST=$(aws s3 ls s3://appcontrol-backups/daily/ --recursive |
  sort | tail -1 | awk '{print $4}')

aws s3 cp "s3://appcontrol-backups/$LATEST" /tmp/latest.dump

# Wipe and restore the sandbox DB
PGPASSWORD=$SANDBOX_PASS psql -h sandbox-db.internal -U postgres -c "DROP DATABASE IF EXISTS appcontrol_test;"
PGPASSWORD=$SANDBOX_PASS psql -h sandbox-db.internal -U postgres -c "CREATE DATABASE appcontrol_test;"
PGPASSWORD=$SANDBOX_PASS pg_restore -h sandbox-db.internal -U postgres \
    -d appcontrol_test /tmp/latest.dump

# Validation queries
COUNT=$(PGPASSWORD=$SANDBOX_PASS psql -At -h sandbox-db.internal -U postgres \
    -d appcontrol_test -c "SELECT count(*) FROM action_log;")
echo "action_log rows: $COUNT"

if [ "$COUNT" -lt 100 ]; then
  echo "VALIDATION FAILED: action_log too small" >&2
  exit 1
fi
```

### Encryption at rest

All cron jobs use `--sse aws:kms` to ensure backups are encrypted at rest with a customer-managed KMS key. This satisfies DORA Article 9 (cryptographic protection of data at rest).

### Off-site replication

Daily backups are replicated to a second region via S3 cross-region replication. The replicated region is used for the doomsday scenario ([§9 of Disaster Recovery](DISASTER_RECOVERY.md#9-doomsday-primary--dr-simultaneously-lost)).

```hcl
resource "aws_s3_bucket_replication_configuration" "appcontrol_backups" {
  bucket = aws_s3_bucket.backups_primary.id
  role   = aws_iam_role.replication.arn

  rule {
    id     = "replicate-all"
    status = "Enabled"
    destination {
      bucket        = aws_s3_bucket.backups_secondary.arn
      storage_class = "GLACIER_IR"
    }
  }
}
```

---

## Reference

- [Disaster Recovery](DISASTER_RECOVERY.md) — scenarios that consume these backups
- [Hardening](HARDENING.md) — backup-encryption requirements
- [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md) — regulatory retention requirements
- PostgreSQL docs — [`pg_dump`](https://www.postgresql.org/docs/16/app-pgdump.html), [continuous archiving](https://www.postgresql.org/docs/16/continuous-archiving.html)
