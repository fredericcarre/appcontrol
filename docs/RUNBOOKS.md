# Operational Runbooks

Incident response procedures for AppControl production environments.

---

## Runbook 1: Agent Not Connecting

**Symptoms**: Agent shows as UNREACHABLE in the dashboard. No heartbeat events.

**Diagnosis**:

```bash
# 1. Check agent process on the host
systemctl status appcontrol-agent
journalctl -u appcontrol-agent --since "30 min ago"

# 2. Check network connectivity to gateway
curl -k https://gateway.your-domain.com:4443/health

# 3. Check TLS certificate validity
openssl x509 -in /etc/appcontrol/agent.crt -noout -dates
openssl verify -CAfile /etc/appcontrol/ca.crt /etc/appcontrol/agent.crt

# 4. Check gateway logs for rejected connections
kubectl logs -n appcontrol -l app.kubernetes.io/component=gateway --tail=50

# 5. Check backend for agent registration
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s localhost:3000/api/v1/agents | jq '.[] | select(.hostname=="the-host")'
```

**Resolution**:

| Cause | Fix |
|-------|-----|
| Agent process down | `systemctl restart appcontrol-agent` |
| Certificate expired | Regenerate cert signed by CA, restart agent |
| Network firewall | Allow TCP 4443 from agent to gateway |
| Gateway down | Check gateway pods: `kubectl get pods -n appcontrol -l app.kubernetes.io/component=gateway` |
| DNS resolution | Verify agent can resolve gateway hostname |

---

## Runbook 2: Backend High CPU / Slow Responses

**Symptoms**: API latency > 2s, Prometheus `http_request_duration_seconds` P95 elevated.

**Diagnosis**:

```bash
# 1. Check pod resource usage
kubectl top pods -n appcontrol -l app.kubernetes.io/component=backend

# 2. Check database connection pool
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s localhost:3000/metrics | grep db_pool

# 3. Check PostgreSQL for slow queries
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "SELECT pid, now()-query_start AS duration, query FROM pg_stat_activity WHERE state='active' AND query_start < now() - interval '5 seconds' ORDER BY duration DESC;"

# 4. Check for table bloat (check_events grows fast)
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "SELECT relname, pg_size_pretty(pg_total_relation_size(oid)) FROM pg_class WHERE relname LIKE 'check_events%' ORDER BY pg_total_relation_size(oid) DESC LIMIT 10;"

# 5. Check active operations (stuck locks?)
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s localhost:3000/metrics | grep http_requests
```

**Resolution**:

| Cause | Fix |
|-------|-----|
| Too many concurrent operations | Check operation lock conflicts in logs |
| Database slow queries | Add missing indexes, run `ANALYZE` |
| check_events table too large | Reduce `RETENTION_CHECK_EVENTS_DAYS`, drop old partitions |
| Connection pool exhausted | Increase `DATABASE_MAX_CONNECTIONS` env var |
| Memory pressure | Increase backend `resources.limits.memory` |

---

## Runbook 3: Database Partition Overflow

**Symptoms**: INSERT errors on `check_events`, logs show "no partition for value".

**Diagnosis**:

```bash
# Check existing partitions
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "SELECT tablename FROM pg_tables WHERE tablename LIKE 'check_events_y%' ORDER BY tablename;"

# Check if current month partition exists
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "SELECT tablename FROM pg_tables WHERE tablename = 'check_events_y2026m02';"
```

**Resolution**:

```bash
# Create missing partition manually
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "CREATE TABLE IF NOT EXISTS check_events_y2026m02 PARTITION OF check_events FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');"

# Also create next month
kubectl exec -n appcontrol appcontrol-postgresql-0 -- \
  psql -U appcontrol -c "CREATE TABLE IF NOT EXISTS check_events_y2026m03 PARTITION OF check_events FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');"

# Backend auto-creates partitions daily. If this keeps happening,
# check the partition maintenance task in backend logs:
kubectl logs -n appcontrol deploy/appcontrol-backend | grep "Partition maintenance"
```

---

## Runbook 4: Token Revocation for Compromised Account

**Symptoms**: Suspected unauthorized access via a compromised JWT token.

**Actions**:

```bash
# 1. Immediately revoke all tokens for the user via API
# (requires admin API key or break-glass access)
curl -X POST https://appcontrol.your-domain.com/api/v1/auth/revoke-all \
  -H "Authorization: Bearer ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "UUID_OF_COMPROMISED_USER"}'

# 2. If Redis is available, verify the revocation entry exists
kubectl exec -n appcontrol deploy/appcontrol-redis-0 -- \
  redis-cli KEYS "revoked:*"

# 3. If Redis is down, rotate the JWT_SECRET to invalidate ALL tokens
kubectl create secret generic appcontrol-jwt \
  --namespace appcontrol \
  --from-literal=jwt-secret="$(openssl rand -base64 64)" \
  --dry-run=client -o yaml | kubectl apply -f -

# 4. Restart backend to pick up new secret
kubectl rollout restart -n appcontrol deployment/appcontrol-backend

# 5. Check audit log for the compromised user's actions
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s "localhost:3000/api/v1/audit?user_id=UUID_OF_COMPROMISED_USER&limit=100"
```

---

## Runbook 5: Redis Failure — Graceful Degradation

**Symptoms**: Redis connection errors in backend logs. Token revocation and caching unavailable.

**Diagnosis**:

```bash
# 1. Check Redis pod status
kubectl get pods -n appcontrol -l app.kubernetes.io/component=redis

# 2. Check Redis connectivity from backend
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s localhost:3000/metrics | grep redis

# 3. Check Redis logs
kubectl logs -n appcontrol -l app.kubernetes.io/component=redis --tail=50
```

**Impact**: Backend continues operating without Redis (fail-open design):
- Token revocation blacklist unavailable (tokens valid until JWT expiry)
- Cache misses increase database load
- Rate limiting counters reset

**Resolution**:

| Cause | Fix |
|-------|-----|
| Redis OOM | Increase `redis.maxmemory`, check eviction policy |
| Redis pod crash | Check PVC for disk issues, restart StatefulSet |
| Network partition | Check NetworkPolicy allows backend → Redis on port 6379 |
| Managed Redis failover | Wait for automatic failover (< 30s), backend reconnects |

---

## Runbook 6: DR Switchover Stuck in Phase

**Symptoms**: Switchover operation started but stuck (e.g., in DRAINING phase for > 10 minutes).

**Diagnosis**:

```bash
# 1. Check switchover status
curl -s https://appcontrol.your-domain.com/api/v1/apps/APP_ID/switchover/status \
  -H "Authorization: Bearer TOKEN" | jq

# 2. Check switchover_log for the latest entries
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s "localhost:3000/api/v1/apps/APP_ID/switchover/log" | jq

# 3. Check if agents on DR site are connected
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s "localhost:3000/api/v1/agents?zone=dr-site" | jq
```

**Resolution**:

```bash
# Force-cancel the stuck switchover (requires manage permission)
curl -X POST https://appcontrol.your-domain.com/api/v1/apps/APP_ID/switchover/cancel \
  -H "Authorization: Bearer TOKEN"

# If the cancel itself is stuck, check for operation locks
kubectl logs -n appcontrol deploy/appcontrol-backend | grep "operation_lock"
```
