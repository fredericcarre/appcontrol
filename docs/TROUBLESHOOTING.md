# Troubleshooting Guide

This page lists the most common production issues with AppControl, their likely causes, and concrete resolution steps. Each entry is structured as **symptom → diagnosis commands → resolution**.

For step-by-step incident-response procedures, see also [Operational Runbooks](RUNBOOKS.md). For DR-specific scenarios, see [Disaster Recovery](DISASTER_RECOVERY.md).

## Table of contents

- [Agent issues](#agent-issues)
- [Gateway issues](#gateway-issues)
- [Backend issues](#backend-issues)
- [Authentication issues](#authentication-issues)
- [FSM issues](#fsm-issues)
- [Operation issues](#operation-issues)
- [Discovery / auto-discovery noise](#discovery--auto-discovery-noise)
- [Cluster (fan-out) sync issues](#cluster-fan-out-sync-issues)
- [DR switchover stuck in phase X](#dr-switchover-stuck-in-phase-x)
- [Diagnostic + Rebuild rejected](#diagnostic--rebuild-rejected)
- [How to file a bug report](#how-to-file-a-bug-report)

---

## Agent issues

### Agent will not connect to gateway

!!! note "Symptom"
    The Agents page shows the agent as `UNREACHABLE`. No heartbeat events are received. `systemctl status appcontrol-agent` reports the binary is running.

**Diagnosis:**

```bash
# 1. Are reconnection attempts visible in the agent journal?
sudo journalctl -u appcontrol-agent --since "5 min ago" | grep -E "connect|gateway|tls"

# 2. Can the agent host reach the gateway TCP port?
nc -vz gateway.example.com 4443

# 3. Does the TLS handshake succeed?
openssl s_client -connect gateway.example.com:4443 \
  -cert /etc/appcontrol/tls/agent.crt \
  -key /etc/appcontrol/tls/agent.key \
  -CAfile /etc/appcontrol/tls/ca.crt -showcerts < /dev/null
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Network firewall blocks 4443 | Open outbound TCP 4443 from the agent subnet to the gateway |
| Wrong gateway URL | Edit `/etc/appcontrol/agent.yaml`, fix `gateway.url` or `gateway.urls`, `systemctl reload appcontrol-agent` |
| DNS resolution failure | `getent hosts gateway.example.com` — if empty, fix `/etc/resolv.conf` or `/etc/hosts` |
| Gateway pod down | `kubectl get pods -n appcontrol -l app.kubernetes.io/component=gateway`; restart if needed |
| `tls_insecure: false` against self-signed gateway cert | Use a CA-signed gateway cert, or set `gateway.tls_insecure: true` (dev only) |

### Enrollment fails

!!! note "Symptom"
    `appcontrol-agent --enroll …` exits non-zero. Returns HTTP 401, 409, or 500.

**Resolution:**

| HTTP code | Cause | Fix |
|----------:|-------|-----|
| 401 | Token invalid or revoked | Generate a new token: `appctl pki create-token --name <n> --max-uses 1 --scope agent` |
| 409 | Token `max_uses` exhausted | Generate a new token with a higher `max_uses` |
| 410 | Token expired | Generate a new token; defaults to 24 h validity |
| 500 | PKI not initialized | `appctl pki init --org-name "<your org>"` |
| Connection refused | Gateway unreachable | Verify `https://gateway:4443/health` is reachable; check the gateway pod |
| `Certificate verify failed` | Agent presented a wrong CA | Re-run enrollment which writes the fresh CA to `/etc/appcontrol/tls/ca.crt` |

See [Agent Installation §8](AGENT_INSTALLATION.md#8-troubleshooting) for the full enrollment-error table.

### Heartbeat timeout (components flip to `UNREACHABLE`)

!!! warning "Symptom"
    Components managed by an apparently healthy agent transition to `UNREACHABLE` periodically. The agent process is up but the backend marks it stale.

**Diagnosis:**

```bash
# 1. Confirm the agent is still sending heartbeats
sudo journalctl -u appcontrol-agent | grep -i heartbeat | tail -10

# 2. Compare the configured timeout
psql $DATABASE_URL -c "SELECT name, heartbeat_timeout_seconds FROM organizations;"

# 3. Check connection latency between agent and gateway
sudo journalctl -u appcontrol-agent | grep -E "rtt|latency" | tail -5
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Network is dropping idle WebSocket connections | Reduce keep-alive interval on the load balancer to < 60 s; or raise heartbeat frequency |
| `heartbeat_timeout_seconds` too low for the network | `UPDATE organizations SET heartbeat_timeout_seconds = 300;` (default is 180 s) |
| Backend `heartbeat_monitor` is over-aggressive | Check backend logs for `marking agent stale` — verify clock skew with NTP |
| Agent log shows repeated `pong timeout` | A firewall is killing idle WS frames — switch the gateway to port 443 (passes as HTTPS) |

### Certificate expired

!!! danger "Symptom"
    Agent journal shows `certificate has expired` or `unable to get local issuer certificate`. The agent does not reconnect.

**Diagnosis:**

```bash
# Check the agent cert expiry
openssl x509 -in /etc/appcontrol/tls/agent.crt -noout -dates

# Check the CA cert expiry
openssl x509 -in /etc/appcontrol/tls/ca.crt -noout -dates
```

**Resolution:**

1. Generate a new enrollment token: `appctl pki create-token --name renewal --max-uses 1 --scope agent`.
2. Stop the agent: `sudo systemctl stop appcontrol-agent`.
3. Re-enroll: `sudo appcontrol-agent --enroll https://gateway:4443 --token ac_enroll_...`.
4. Start the agent: `sudo systemctl start appcontrol-agent`.

If the CA itself expired, see [Disaster Recovery — CA loss](DISASTER_RECOVERY.md).

### Advisory mode — agent reports but does not execute

!!! note "Symptom"
    Operator clicks **Start** but nothing happens on the host. The map updates check status but no command runs.

**Diagnosis:**

```bash
# 1. Is the application or agent in advisory mode?
psql $DATABASE_URL -c "SELECT id, name, settings->>'mode' FROM applications WHERE id = '<app_id>';"

# 2. Check the action log — operations should be recorded even if no-op
psql $DATABASE_URL -c "SELECT action_type, details FROM action_log WHERE target_id='<app_id>' ORDER BY created_at DESC LIMIT 5;"
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Application is in `advisory` mode | Toggle to `operate` mode in **Settings > Application > Mode** |
| Component has `is_optional=true` and was skipped | Inspect the sequencer log; flip `is_optional=false` if required |
| User has only `view` permission | Grant `operate` permission via the Share dialog |

---

## Gateway issues

### Gateway down

!!! danger "Symptom"
    All agents in a zone go `UNREACHABLE` at once. The gateway pod is crash-looping or not reachable.

**Diagnosis:**

```bash
# 1. Pod state
kubectl get pods -n appcontrol -l app.kubernetes.io/component=gateway

# 2. Logs
kubectl logs -n appcontrol -l app.kubernetes.io/component=gateway --tail=100

# 3. Health endpoint (from outside the cluster)
curl -k https://gateway.example.com:4443/health
# Returns: ok agents=42 backend=connected buffer_msgs=0 buffer_bytes=0
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Backend WebSocket URL wrong | Edit `BACKEND_URL` env var, must end with `/ws/gateway` |
| TLS cert / key mismatch | Verify file paths and `openssl rsa -in gateway.key -modulus` matches the cert |
| Listening port already taken | Check `ss -tlnp | grep 4443`; change `LISTEN_PORT` if conflicting |
| OOM kill | Increase `resources.limits.memory` in the Helm values |

### mTLS handshake failure

!!! warning "Symptom"
    Gateway logs show repeated `tls handshake failure` or `unknown ca`. Agents cannot connect.

**Diagnosis:**

```bash
# From the agent host, simulate the full handshake
openssl s_client -connect gateway.example.com:4443 \
  -cert /etc/appcontrol/tls/agent.crt \
  -key /etc/appcontrol/tls/agent.key \
  -CAfile /etc/appcontrol/tls/ca.crt -verify_return_error

# Compare the issuer fingerprint of the gateway's chain and the agent's CA
openssl x509 -in /etc/appcontrol/tls/ca.crt -noout -fingerprint -sha256
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Gateway and agent use different CAs | Re-enroll the agent against the current backend PKI |
| Gateway cert SAN does not include the DNS the agent uses | Reissue the gateway cert with the right DNS SANs |
| Clock skew > 5 min | Sync NTP on both ends; certs are sensitive to clock drift |
| `TLS_ENABLED=false` on one side | Verify both gateway and agent have `tls.enabled: true` |

### Rate-limited

!!! note "Symptom"
    Gateway returns HTTP 429 on `/enroll`, or per-agent log spam shows "rate limit exceeded".

**Resolution:**

```bash
# 1. Identify the offending agent
kubectl logs -n appcontrol -l app.kubernetes.io/component=gateway | grep "rate limit" | awk '{print $NF}' | sort | uniq -c

# 2. The agent is likely sending too many CheckResults — investigate why deltas are flapping
psql $DATABASE_URL -c "
  SELECT component_id, count(*) FROM check_events
  WHERE created_at > now() - interval '5 minutes'
  GROUP BY component_id ORDER BY count(*) DESC LIMIT 10;"
```

If the rate is legitimate (large cluster of components on one agent), raise the per-agent rate limit in the gateway config; otherwise fix the flapping `check_cmd`.

---

## Backend issues

### 5xx errors

!!! danger "Symptom"
    The frontend shows "Internal Server Error". `curl /api/v1/apps` returns HTTP 500/502/503.

**Diagnosis:**

```bash
# 1. Backend pod state
kubectl get pods -n appcontrol -l app.kubernetes.io/component=backend
kubectl logs -n appcontrol -l app.kubernetes.io/component=backend --tail=200

# 2. Database reachability
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s http://localhost:3000/ready

# 3. Recent panics
kubectl logs -n appcontrol -l app.kubernetes.io/component=backend | grep -E "panicked|FATAL|thread"
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| DB unreachable | See [DB pool exhausted](#db-pool-exhausted) and `/ready` |
| Panic on startup (insecure JWT in production) | Set a 32+ char `JWT_SECRET` via `openssl rand -base64 48` |
| Missing migration | Check `kubectl logs` for `sqlx::migrate` errors; investigate the partial migration |
| Unhandled JSON deserialization error | Capture the request payload, file a bug report |

### DB pool exhausted

!!! warning "Symptom"
    Backend logs show `acquire timeout` or `pool timed out while waiting for an open connection`. P95 latency spikes.

**Diagnosis:**

```bash
# 1. Current pool size and utilization
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -s localhost:3000/metrics | grep db_pool

# 2. Long-running queries in PostgreSQL
psql $DATABASE_URL -c "
  SELECT pid, now() - query_start AS duration, state, query
  FROM pg_stat_activity WHERE state = 'active' AND query_start < now() - interval '2 seconds'
  ORDER BY duration DESC LIMIT 10;"
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Long-running query holds a connection | Identify the query, add an index, or rewrite |
| Pool too small | Raise `DB_POOL_SIZE` (default 20; rule of thumb: 10 + apps × 0.5, capped at 50) |
| Idle connection timeout > LB idle timeout | Set `DB_IDLE_TIMEOUT_SECS` below the LB idle limit (e.g. AWS ALB = 350) |
| Connection leak in a custom handler | Review recent backend changes for missing `drop` on a tx |

### Slow queries

!!! note "Symptom"
    P95 API latency above 2 s. Specific endpoints (`/apps`, `/applications/:id`) feel sluggish.

**Diagnosis:**

```sql
-- Identify the slowest queries
SELECT
  substring(query, 1, 80) AS query,
  calls, total_exec_time, mean_exec_time
FROM pg_stat_statements
ORDER BY mean_exec_time DESC LIMIT 15;

-- Check check_events partition sizes
SELECT
  inhrelid::regclass AS partition,
  pg_size_pretty(pg_total_relation_size(inhrelid)) AS size
FROM pg_inherits WHERE inhparent = 'check_events'::regclass
ORDER BY pg_total_relation_size(inhrelid) DESC LIMIT 10;
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| `check_events` partition not pruned | Set `RETENTION_CHECK_EVENTS_DAYS=90` and let background task drop old partitions |
| Missing index on a custom query | `EXPLAIN ANALYZE` then `CREATE INDEX` |
| Sequential scan on `state_transitions` | Verify `idx_state_transitions_component_created` exists |
| Tablespace fragmentation | `VACUUM ANALYZE state_transitions;` (off-hours) |

### `JWT_SECRET` insecure panic

!!! danger "Symptom"
    Backend exits at startup with `FATAL: JWT_SECRET must be set to a strong value in production`.

**Resolution:**

```bash
# Generate a strong secret
NEW=$(openssl rand -base64 48)

# Update the Kubernetes secret
kubectl create secret generic appcontrol-jwt \
  --namespace appcontrol \
  --from-literal=jwt-secret="$NEW" \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart the backend
kubectl rollout restart -n appcontrol deployment/appcontrol-backend
```

Note: rotating `JWT_SECRET` invalidates **all** active sessions — users must log in again.

### CORS rejects browser request

!!! note "Symptom"
    The browser console shows `CORS policy: No 'Access-Control-Allow-Origin' header`.

**Resolution:**

`CORS_ORIGINS` must explicitly list the frontend origin in production. Example:

```bash
CORS_ORIGINS=https://appcontrol.example.com,https://admin.example.com
```

In development (`APP_ENV=development`), an empty `CORS_ORIGINS` is permissive. In production, an empty value rejects **all** cross-origin requests by design.

---

## Authentication issues

### OIDC discovery failed

!!! warning "Symptom"
    Backend logs `failed to fetch OIDC discovery document`. Login redirect returns 500.

**Diagnosis:**

```bash
# 1. Verify the discovery URL is reachable from the backend
kubectl exec -n appcontrol deploy/appcontrol-backend -- \
  curl -sv https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration

# 2. Check the variable is set
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep OIDC
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Wrong URL | Fix `OIDC_DISCOVERY_URL` (the realm path matters) |
| Network egress blocked | Allow egress from the backend pods to the IdP |
| Self-signed IdP cert | Mount the IdP's CA into the backend container's trust store |

### SAML assertion invalid

!!! warning "Symptom"
    SAML POST to `/api/v1/auth/saml/acs` returns `invalid signature` or `assertion expired`.

**Diagnosis:**

Inspect the SAMLResponse with [https://samltool.com](https://samltool.com) (paste the base64 from the network tab).

**Resolution:**

| Cause | Fix |
|-------|-----|
| Wrong IdP signing cert | Re-fetch and base64-encode the IdP cert into `SAML_IDP_CERT` |
| Clock skew | Sync NTP; SAML enforces a 5-min `NotBefore`/`NotOnOrAfter` window |
| `SAML_WANT_ASSERTIONS_SIGNED=true` but IdP sends unsigned assertion | Either enable signed assertions in IdP or set the env var to `false` (not recommended) |
| `SAML_SP_ACS_URL` mismatch | The IdP's "Reply URL" must equal exactly the backend ACS URL |

### API key expired

!!! note "Symptom"
    Scheduler integration receives HTTP 401 with `key expired`.

**Resolution:**

```bash
# 1. Identify the offending key
psql $DATABASE_URL -c "
  SELECT id, name, expires_at FROM api_keys
  WHERE user_id = '<user_id>' AND expires_at < now();"

# 2. Create a new key via the UI or API; copy the plaintext immediately
curl -X POST https://appcontrol.example.com/api/v1/api-keys \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"name":"scheduler-2026-q2","expires_in_days":365}'

# 3. Update the scheduler configuration with the new key
```

### "Forbidden" on a resource you should own

!!! warning "Symptom"
    User has been granted `manage` on an application but still gets HTTP 403.

**Diagnosis:**

```sql
-- What permissions does the user have (direct + team)?
SELECT permission_level, source FROM (
  SELECT permission_level, 'direct' AS source FROM app_permissions_users
  WHERE user_id = '<uid>' AND app_id = '<aid>' AND (expires_at IS NULL OR expires_at > now())
  UNION ALL
  SELECT pt.permission_level, 'team:' || t.name FROM app_permissions_teams pt
  JOIN team_members tm ON tm.team_id = pt.team_id
  JOIN teams t ON t.id = pt.team_id
  WHERE tm.user_id = '<uid>' AND pt.app_id = '<aid>'
    AND (pt.expires_at IS NULL OR pt.expires_at > now())
) x;
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Permission has `expires_at` in the past | Update or remove the `expires_at` |
| Team membership missing | Add the user to the team |
| Workspace restriction excludes the site | Add the user (or their team) to a workspace that includes the site |
| Stale JWT cached | User signs out and back in to refresh claims |

---

## FSM issues

### Component stuck in `STARTING`

!!! warning "Symptom"
    The map shows a component blue-pulsing in `STARTING` for minutes. The sequencer waits forever.

**Diagnosis:**

```bash
# 1. Is the start command still running on the host?
ssh <host> ps -ef | grep -v grep | grep "<expected process>"

# 2. Latest health check
psql $DATABASE_URL -c "
  SELECT exit_code, stdout, duration_ms, created_at
  FROM check_events
  WHERE component_id = '<cid>' ORDER BY created_at DESC LIMIT 5;"
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| `start_cmd` returned but process actually died | Fix the start command; ensure the process daemonizes correctly |
| `check_cmd` checks a different process | Realign check and start commands |
| `start_timeout_seconds` too short | Raise it on the component config (default 120 s) |
| Health check polled too infrequently | Sequencer polls every 3 s during transitions — usually fine; if not, file a bug |

### Stuck in `UNREACHABLE` after agent reconnect

!!! note "Symptom"
    Agent reconnects and heartbeats again, but its components remain `UNREACHABLE`.

**Diagnosis:**

```bash
# 1. Check the last 5 state transitions
psql $DATABASE_URL -c "
  SELECT previous_state, next_state, trigger, created_at
  FROM state_transitions WHERE component_id='<cid>'
  ORDER BY created_at DESC LIMIT 5;"

# 2. Force a fresh health check
curl -X POST https://appcontrol.example.com/api/v1/components/<cid>/check \
  -H "Authorization: Bearer $TOKEN"
```

**Resolution:**

The FSM expects a fresh `CheckResult` to leave `UNREACHABLE`. Trigger a manual check (above). If the issue recurs, see [Runbook 1](RUNBOOKS.md#runbook-1-agent-not-connecting).

### State oscillates between `RUNNING` and `FAILED`

!!! danger "Symptom"
    A component flips state every 30 s. Operators see "flapping" alerts.

**Diagnosis:**

```sql
-- Inspect recent transitions
SELECT previous_state, next_state, trigger, created_at, details
FROM state_transitions WHERE component_id = '<cid>'
ORDER BY created_at DESC LIMIT 20;

-- Inspect recent check results
SELECT exit_code, duration_ms, substring(stdout, 1, 80) AS stdout
FROM check_events WHERE component_id = '<cid>'
ORDER BY created_at DESC LIMIT 20;
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| `check_cmd` is non-deterministic | Make the check idempotent and stable |
| Timeout on check | Raise the agent-level command timeout; default is `check_interval_seconds` |
| Resource exhaustion on host | Inspect CPU/RAM; the host may be killing the process under load |
| Race between two agents managing the same process | Verify one agent per component; remove duplicates |

---

## Operation issues

### Start times out

!!! warning "Symptom"
    A start operation runs the configured `start_cmd` but never completes. The orchestrator marks the operation timed-out.

**Resolution:**

1. Inspect the action log: `SELECT * FROM action_log WHERE target_id='<app_id>' ORDER BY created_at DESC LIMIT 5;`.
2. The `details` JSON shows which component failed and the timeout that triggered.
3. Common fix: raise `start_timeout_seconds` on the offending component (default 120 s).
4. If many components share the same issue, raise the app-level timeout in **Settings > Operations**.

### Stop hangs

!!! note "Symptom"
    A stop operation is stuck — components remain in `STOPPING`.

**Diagnosis:**

```bash
# Is the stop process still running?
ssh <host> ps -ef | grep "<process>"

# Did the agent receive the command?
sudo journalctl -u appcontrol-agent | grep -E "ExecuteCommand|stop" | tail -5
```

**Resolution:**

- The component's `stop_cmd` may be waiting for graceful shutdown that never completes. Use the **Force Stop** action in the UI, which sends `kill -9` after the timeout.
- If `stop_timeout_seconds` is too short, raise it on the component (default 60 s).

### Sequencer suspends mid-DAG

!!! note "Symptom"
    Start operation runs the first DAG levels successfully then halts. UI shows "Operation suspended — operator intervention required".

**Resolution:**

The sequencer **suspends** (rather than cancels) on a component failure to give operators a chance to fix the underlying issue without losing in-flight progress. Two next steps:

1. Inspect the failed component, fix the root cause (often a missing prereq or a bad command).
2. Resume the operation from the UI ("Resume" button) or via API: `POST /api/v1/operations/:id/resume`.

If you cannot resume, cancel the operation: `POST /api/v1/operations/:id/cancel` and start fresh.

### Error branch not detected

!!! note "Symptom"
    A component fails, but the "Restart error branch" button targets only the failed component — not its downstream dependents.

**Resolution:**

The error-branch algorithm walks **downstream** (dependent of failed → dependent of dependent…). If your topology has weak coupling that AppControl does not yet treat as dependencies, the dependents are not reached.

- Verify the `dependencies` rows for the failed component: `SELECT to_component_id FROM dependencies WHERE from_component_id = '<failed_cid>';`.
- If a dependency is missing, add it via the map editor (drag an edge between components).

### Dry-run says OK but actual run fails

!!! warning "Symptom"
    `appctl start <app> --dry-run` reports a clean plan. The same operation without `--dry-run` fails.

**Resolution:**

Dry-run validates:

- DAG (cycles, missing references)
- Permissions
- Agent connectivity
- Command syntax (presence, not semantics)

Dry-run does **not** execute commands, so it cannot detect:

- Wrong credentials in `start_cmd`
- Missing prerequisites on the host (port already in use, disk full)
- Race conditions between components

For tighter pre-flight validation, run the diagnostic engine (Level 3) before a critical operation: `appctl diagnose <app> --level 3`.

---

## Discovery / auto-discovery noise

!!! note "Symptom"
    Auto-discovery surfaces hundreds of irrelevant processes (cron jobs, OS daemons), making the draft topology unusable.

**Resolution:**

Discovery is heuristic. Two ways to tame it:

1. **Filter at the agent** — edit `agent.yaml` to set `discovery.exclude_processes: [crond, rsyslogd, ...]` (currently configured via env vars: `APPCONTROL_DISCOVERY_EXCLUDE=crond,rsyslogd,sshd`). _(verify in code; was not found at audit time)_
2. **Curate in the UI** — discovery presents a draft topology; uncheck any process you do not want before promoting it to an application.

If discovery produces no signal at all, check that the agent has permission to read `/proc` (Linux) or `tasklist` (Windows). Container agents may need `--pid=host` to see host processes.

---

## Cluster (fan-out) sync issues

!!! warning "Symptom"
    A fan-out cluster shows divergent member states or a parent state that does not reflect members.

**Diagnosis:**

```sql
-- Are all members reporting check results?
SELECT cm.hostname, cms.current_state, cms.last_check_at
FROM cluster_members cm
LEFT JOIN cluster_member_state cms ON cms.cluster_member_id = cm.id
WHERE cm.component_id = '<cid>' AND cm.is_enabled = true;

-- Does the parent component's state match the policy?
SELECT id, name, cluster_mode, cluster_health_policy, cluster_min_healthy_pct, current_state
FROM components WHERE id = '<cid>';
```

**Resolution:**

| Cause | Fix |
|-------|-----|
| Member's `agent_id` is wrong | Update `cluster_members.agent_id`; verify the agent is reachable |
| `cluster_health_policy = threshold_pct` but `cluster_min_healthy_pct` too high | Lower the threshold or fix the failing members |
| Member's `check_cmd_override` is wrong | Test the override on the host; correct as needed |
| Member's `is_enabled = false` is skipping live nodes | Re-enable: `UPDATE cluster_members SET is_enabled = true WHERE id = '<mid>';` |

---

## DR switchover stuck in phase X

!!! danger "Symptom"
    A switchover starts but stays in phase 1–6 for an unusually long time.

**Resolution by phase:**

| Phase | Likely cause | Fix |
|------:|--------------|-----|
| 1 (Prepare) | Target site agents not connected | `kubectl logs gateway-dr ...`; reconnect agents |
| 2 (Validate) | Pre-flight health check failed on target | Inspect `switchover_log.details`; fix the failing component |
| 3 (StopSource) | Source component's `stop_cmd` hangs | Raise `stop_timeout_seconds`, or "force stop" |
| 4 (Sync) | Data replication lag exceeds threshold | Wait for replication, or trigger an emergency sync |
| 5 (StartTarget) | Target component fails to start | Fix the host issue, then resume |
| 6 (Commit) | Database write contention | Should never linger; if it does, file a bug report |

Inspect:

```sql
SELECT phase, status, started_at, ended_at, details
FROM switchover_log
WHERE application_id = '<aid>'
ORDER BY started_at DESC LIMIT 20;
```

To force-cancel (only before phase 6): `POST /api/v1/apps/:id/switchover/cancel`. See [Runbook 6](RUNBOOKS.md#runbook-6-dr-switchover-stuck-in-phase).

---

## Diagnostic + Rebuild rejected

!!! warning "Symptom"
    `POST /api/v1/apps/:id/rebuild` returns HTTP 409 `rebuild_protected`.

**Resolution:**

The rebuild engine refuses to touch any component flagged `rebuild_protected = true`. This is intentional protection for databases of record and other irreplaceable components.

Options:

1. Rebuild the application **excluding** the protected components — pass `exclude_component_ids` in the rebuild request body.
2. Temporarily unprotect a component (requires `manage` permission):

```sql
UPDATE components SET rebuild_protected = false WHERE id = '<cid>';
```

Re-protect after the rebuild finishes. The change is recorded in `config_versions`.

---

## How to file a bug report

When opening an issue, attach the following so the maintainer can reproduce:

### 1. Environment

```bash
# Version of every component
appctl version
kubectl exec -n appcontrol deploy/appcontrol-backend -- /app/appcontrol-backend --version
kubectl exec -n appcontrol deploy/appcontrol-gateway -- /app/appcontrol-gateway --version
appcontrol-agent --version

# Database flavor
psql $DATABASE_URL -c "SELECT version();"
```

### 2. Logs (last 200 lines from each)

```bash
kubectl logs -n appcontrol deploy/appcontrol-backend --tail=200 > backend.log
kubectl logs -n appcontrol -l app.kubernetes.io/component=gateway --tail=200 > gateway.log
sudo journalctl -u appcontrol-agent --since "1 hour ago" > agent.log
```

### 3. Configuration (redact secrets)

```bash
# Backend env (redact JWT_SECRET, DATABASE_URL password)
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep -E '^[A-Z_]+=' | grep -v -E 'SECRET|PASSWORD|KEY|DATABASE_URL'

# Gateway YAML
kubectl exec -n appcontrol deploy/appcontrol-gateway -- cat /etc/appcontrol/gateway.yaml

# Agent YAML
cat /etc/appcontrol/agent.yaml
```

### 4. Minimal reproduction

A 3-step recipe that triggers the bug on a fresh `docker compose -f docker/docker-compose.release.yaml up -d` install. If the bug requires a specific application topology, attach the JSON export from **Settings > Application > Export**.

### 5. Database state (optional, helpful)

```sql
-- Component, last 5 transitions, last 5 check events
SELECT id, name, current_state FROM components WHERE id = '<cid>';
SELECT previous_state, next_state, trigger, created_at FROM state_transitions
  WHERE component_id = '<cid>' ORDER BY created_at DESC LIMIT 5;
SELECT exit_code, substring(stdout, 1, 100), created_at FROM check_events
  WHERE component_id = '<cid>' ORDER BY created_at DESC LIMIT 5;
```

### 6. File log paths reference

| Component | Log path (Linux) | Log path (Kubernetes) |
|-----------|------------------|-----------------------|
| Agent | `journalctl -u appcontrol-agent` | n/a (runs outside K8s) |
| Gateway | `journalctl -u appcontrol-gateway` (when on a VM) | `kubectl logs -l app.kubernetes.io/component=gateway` |
| Backend | n/a (typically Kubernetes) | `kubectl logs -l app.kubernetes.io/component=backend` |
| Frontend (nginx) | `/var/log/nginx/` inside the container | `kubectl logs -l app.kubernetes.io/component=frontend` |
| PostgreSQL | `/var/log/postgresql/` | depends on managed-service provider |

Submit the bundle to the project issue tracker. Reproducible bug reports with attached logs are typically triaged within 48 h.
