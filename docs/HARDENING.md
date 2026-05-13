# Production Hardening Checklist

This page is a literal checklist to walk through before promoting an AppControl deployment to production. Each item gives the rationale and the **verification command** so you can prove compliance.

For the architectural background, see [Security Architecture](SECURITY_ARCHITECTURE.md). For the regulatory mapping, see [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md).

!!! danger "Default config is for development"
    Out of the box, AppControl ships with developer-friendly defaults (`JWT_SECRET=dev-secret-...`, no CORS restriction, `APP_ENV=development`). Production deployments must explicitly opt out of every dev convenience.

---

## Authentication and secrets

### `JWT_SECRET` set to ≥ 32 random characters

**Rationale.** JWTs sign every API call. A weak or default secret lets any attacker forge admin tokens. The backend panics on startup in production when the secret is short or matches known dev values.

**Set.**

```bash
NEW=$(openssl rand -base64 48)
kubectl create secret generic appcontrol-jwt \
  --namespace appcontrol \
  --from-literal=jwt-secret="$NEW" \
  --dry-run=client -o yaml | kubectl apply -f -
kubectl rollout restart -n appcontrol deployment/appcontrol-backend
```

**Verify.**

```bash
# Length ≥ 32
kubectl get secret -n appcontrol appcontrol-jwt -o jsonpath='{.data.jwt-secret}' | base64 -d | wc -c

# Not in known-bad list
SEC=$(kubectl get secret -n appcontrol appcontrol-jwt -o jsonpath='{.data.jwt-secret}' | base64 -d)
[[ "$SEC" =~ (dev|change|secret|password) ]] && echo "FAIL: insecure" || echo "OK"
```

- [ ] **Set and verified.**

### `APP_ENV=production`

**Rationale.** Setting `APP_ENV=production` toggles strict mode: insecure-default warnings become fatal errors, dev login endpoints are disabled, CORS becomes restrictive.

**Verify.**

```bash
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep APP_ENV
# APP_ENV=production
```

- [ ] **APP_ENV=production confirmed in every backend replica.**

### `CORS_ORIGINS` restricted to known frontends

**Rationale.** A permissive CORS policy lets any malicious site script the API on behalf of a logged-in user.

**Set.**

```yaml
backend:
  env:
    CORS_ORIGINS: "https://appcontrol.example.com,https://admin.example.com"
```

**Verify.**

```bash
# Confirm the env is set
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep CORS_ORIGINS

# Attempt a cross-origin request — must be rejected
curl -i -H "Origin: https://attacker.example" https://appcontrol.example.com/api/v1/apps
# Expect: 403 or missing Access-Control-Allow-Origin
```

- [ ] **CORS restricted and verified.**

### OIDC or SAML configured; local auth disabled in production

**Rationale.** Local-password authentication is appropriate for development but should not be the production door. SSO centralizes account lifecycle, MFA enforcement, and SSO-level auditing.

**Set.** Configure OIDC or SAML per [Configuration §Users](CONFIGURATION.md#users--authentication).

**Verify.**

```bash
# In production, /auth/login returns 404
curl -is https://appcontrol.example.com/api/v1/auth/login -X POST -d '{}'
# Expect: HTTP/2 404
```

- [ ] **SSO configured, local auth disabled.**

### API key rotation policy

**Rationale.** Stale API keys widen the attack surface. Rotate every 90 days (industry standard for high-privilege machine credentials).

**Set.** When creating an API key, set `expires_in_days: 90`. Document the rotation procedure in your runbook.

**Verify.**

```sql
-- Keys older than 90 days should be < 5 % of total keys
SELECT
  count(*) FILTER (WHERE created_at < now() - interval '90 days') AS stale,
  count(*) AS total
FROM api_keys WHERE revoked_at IS NULL;
```

- [ ] **Rotation policy documented; stale keys identified and rotated.**

### Enrollment tokens: limited `max_uses` and short `valid_hours`

**Rationale.** An enrollment token grants permanent identity to whoever uses it first. A token with `max_uses: 1000` and `valid_hours: 8760` is effectively a permanent credential.

**Set.** Recommended defaults:

| Use case | `max_uses` | `valid_hours` |
|----------|-----------:|--------------:|
| Onboarding a single agent | 1 | 4 |
| Onboarding a batch of agents (Ansible playbook) | N (exactly) | 4 |
| Long-running automation | not recommended | n/a — prefer per-host tokens |

**Verify.**

```sql
SELECT name, max_uses, expires_at FROM enrollment_tokens
WHERE revoked_at IS NULL AND expires_at > now()
ORDER BY expires_at DESC;
```

- [ ] **All active tokens have bounded `max_uses` and `expires_at` ≤ 7 days.**

---

## Transport security

### mTLS enabled

**Rationale.** Plaintext between agent and gateway exposes commands, credentials, and check output. mTLS provides mutual authentication and forward-secret encryption.

**Set.** Ensure `TLS_ENABLED=true` on every gateway and agent. Certificates come from the per-org CA (auto-PKI) or your enterprise PKI. See [Configuration — TLS](CONFIGURATION.md#tls--mtls-certificate-configuration).

**Verify.**

```bash
# Gateway
kubectl exec -n appcontrol deploy/appcontrol-gateway -- env | grep TLS_ENABLED
# TLS_ENABLED=true

# Agent
grep enabled /etc/appcontrol/agent.yaml
# enabled: true

# Probe the gateway with no cert — must be rejected
curl -k https://gateway.example.com:4443/health 2>&1 | grep -i "alert.*certificate"
# Or simulate a full handshake without client cert — should fail
openssl s_client -connect gateway.example.com:4443 < /dev/null 2>&1 | grep -i "alert"
```

- [ ] **mTLS enabled end-to-end.**

### Rate limits reviewed

**Rationale.** Defaults are reasonable for moderate fleets. Larger or smaller installs need to tune.

| Variable | Default | Rationale |
|----------|--------:|-----------|
| `RATE_LIMIT_AUTH` | 10/min/IP | Prevents brute-force login |
| `RATE_LIMIT_OPERATIONS` | 5/min/user | Throttles per-user start/stop spam |
| `RATE_LIMIT_READS` | 200/min/user | Tolerates dashboard polling |

**Verify.**

```bash
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep RATE_LIMIT

# Trigger a 429
for i in $(seq 1 12); do
  curl -is -X POST https://appcontrol.example.com/api/v1/auth/login -d '{}' | head -1
done
# Expect: 429 after the 10th request
```

- [ ] **Rate limits set appropriate for fleet size.**

---

## PKI and certificate hygiene

### CA private key not on disk where the backend runs

**Rationale.** The CA private key is the master credential. If the backend host is compromised, the attacker can sign any cert.

**Set.** Two acceptable options:

- **HSM / KMS-backed CA.** Configure the backend to call HashiCorp Vault PKI / AWS KMS / Azure Key Vault for signing operations. _(verify in code; was not found at audit time)_
- **Sealed secret with strict RBAC.** Keep the CA in a Kubernetes secret with only the backend ServiceAccount as reader; encrypt the secret at rest with a KMS-backed key.

**Verify.**

```bash
# Verify the backend cannot read the CA from disk
kubectl exec -n appcontrol deploy/appcontrol-backend -- ls -la /etc/appcontrol/pki/ca.key 2>&1
# Expect: No such file or directory
```

- [ ] **CA private key not stored on the backend filesystem.**

### Database credentials are not the default `appcontrol:appcontrol@localhost`

**Rationale.** A default-password database is a common entry point for lateral movement.

**Set.** Use a strong, randomly-generated password. Restrict source IPs at the DB level.

**Verify.**

```bash
# Inspect the database URL — must not contain default password
URL=$(kubectl get secret -n appcontrol appcontrol-db -o jsonpath='{.data.database-url}' | base64 -d)
[[ "$URL" =~ appcontrol:appcontrol@ ]] && echo "FAIL: default creds" || echo "OK"

# Also verify pg_hba.conf accepts only the backend's source
psql $DATABASE_URL -c "SHOW listen_addresses;"
psql $DATABASE_URL -c "SELECT * FROM pg_hba_file_rules WHERE type = 'host';"
```

- [ ] **Database credentials hardened.**

---

## Data retention and audit

### `RETENTION_ACTION_LOG_DAYS` and `RETENTION_CHECK_EVENTS_DAYS` set per regulation

**Rationale.** DORA Article 16 requires incident records for at least 5 years. NIS2 has similar expectations. Defaults are `0` (unlimited) which technically satisfies the rule but exposes you to storage exhaustion. Set explicit values.

**Set.**

```yaml
backend:
  env:
    RETENTION_ACTION_LOG_DAYS:   "1825"   # 5 years
    RETENTION_CHECK_EVENTS_DAYS: "90"     # 3 months hot, then offloaded to archive
```

For check events older than 90 days, archive partitions to cold storage before the retention task drops them (see [Backup & Restore — Partitioned check_events](BACKUP_RESTORE.md#partitioned-check_events)).

**Verify.**

```bash
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep RETENTION
```

- [ ] **Retention values set and reviewed with compliance team.**

### Logs go to a centralized SIEM in JSON format

**Rationale.** Per-pod log files are insufficient evidence for a regulator. Structured logs let you correlate access patterns and detect anomalies.

**Set.**

```yaml
backend:
  env:
    LOG_FORMAT: "json"
    RUST_LOG:   "info,appcontrol_backend=info,tower_http=info"
```

Forward stdout/stderr to your SIEM (Splunk, Datadog, Elastic, Loki).

**Verify.**

```bash
# Recent logs must be valid JSON
kubectl logs -n appcontrol deploy/appcontrol-backend --tail=10 | jq -c '.'
```

- [ ] **JSON logs forwarded to SIEM.**

---

## Backups

### Backups encrypted at rest, with an off-site copy

**Rationale.** Production backups containing the action log are themselves sensitive. They must be encrypted at rest, and an off-site copy must exist for the doomsday scenario.

**Set.** See [Backup & Restore — Encryption at rest](BACKUP_RESTORE.md#encryption-at-rest) for `aws s3 cp --sse aws:kms` example and cross-region replication.

**Verify.**

```bash
# Confirm KMS encryption on the most recent backup
LATEST=$(aws s3 ls s3://appcontrol-backups/daily/ --recursive | sort | tail -1 | awk '{print $4}')
aws s3api head-object --bucket appcontrol-backups --key "$LATEST" | jq '.ServerSideEncryption, .SSEKMSKeyId'

# Confirm cross-region replication
aws s3api get-bucket-replication --bucket appcontrol-backups
```

- [ ] **Backups encrypted; off-site copy verified.**

---

## Network policy

### Agents → gateway only; gateway → backend only; no direct agent → backend

**Rationale.** Defense in depth: even if an agent is compromised, it should not be able to talk directly to the database or to other agents.

**Set.** Kubernetes NetworkPolicies (the Helm chart ships sensible defaults — verify they are enabled).

```yaml
networkPolicy:
  enabled: true
```

This restricts:

| Source | Destination | Ports allowed |
|--------|-------------|---------------|
| Frontend pods | Backend pods | 3000 |
| Ingress controller | Frontend pods | 8080 |
| Backend pods | PostgreSQL | 5432 |
| Backend pods | Gateway pods | 3000 (WS) |
| External (any) | Gateway pods | 4443 |
| Gateway pods | Backend pods | 3000 |
| All others | denied |

**Verify.**

```bash
kubectl get networkpolicy -n appcontrol
kubectl describe networkpolicy -n appcontrol

# From an agent host, try the backend directly — must fail
nc -vz appcontrol-backend.appcontrol.svc.cluster.local 3000
# Expect: connection refused / blocked
```

- [ ] **NetworkPolicies enabled and tested.**

---

## Process and OS hardening

### Backend / gateway / agent run as non-root

**Rationale.** A privilege-escalation bug becomes far more dangerous if the process is already root.

**Set.** Helm chart uses `runAsNonRoot: true` and `runAsUser: 65532` for backend / gateway / frontend. Agent runs as `root` by default on hosts because it executes user-defined commands; restrict to a dedicated `appcontrol` user where you can.

**Verify.**

```bash
# Container UID
kubectl get pod -n appcontrol -l app.kubernetes.io/component=backend -o jsonpath='{.items[0].spec.securityContext.runAsUser}'
# Expect: 65532 or similar non-zero

# Agent user (on host)
ps -o user= -p $(pidof appcontrol-agent)
```

- [ ] **Non-root verified for backend, gateway, frontend.**

### PostgreSQL: `ssl=require`, scram-sha-256 auth

**Rationale.** Plaintext SQL on a public network leaks every query (and the JWT in some). SCRAM-SHA-256 is the strongest standard PostgreSQL auth mechanism.

**Set.**

```
# pg_hba.conf
hostssl appcontrol appcontrol 10.0.0.0/16 scram-sha-256

# postgresql.conf
password_encryption = scram-sha-256
ssl = on
```

```
# In DATABASE_URL
postgres://appcontrol:PASSWORD@db.internal:5432/appcontrol?sslmode=require
```

**Verify.**

```bash
psql $DATABASE_URL -c "SHOW ssl; SHOW password_encryption;"
# ssl                       | on
# password_encryption       | scram-sha-256
```

- [ ] **PostgreSQL TLS + SCRAM enabled.**

### OS hardening: SELinux/AppArmor enforcing; recent patches

**Rationale.** Standard infrastructure hygiene. Mandatory Access Control limits the blast radius of any process compromise.

**Set.** Distribution-dependent. RHEL: `setenforce 1`; Ubuntu: `aa-status` enforcing on `appcontrol-agent`. Apply security errata weekly.

**Verify.**

```bash
# RHEL
getenforce
# Expect: Enforcing

# Ubuntu
sudo aa-status | head -3
# Expect: profiles loaded

# Patches
sudo dnf updateinfo summary security  # RHEL
sudo unattended-upgrade --dry-run     # Ubuntu
```

- [ ] **MAC enforcing; security patches current.**

### No debug toolchain on production hosts

**Rationale.** Compilers, package managers, and debugging tools are the first thing an intruder reaches for. Strip them from production images.

**Verify.**

```bash
# Inside the backend container
kubectl exec -n appcontrol deploy/appcontrol-backend -- sh -c 'which gcc gdb strace tcpdump apt yum 2>&1' | grep -v 'not found' && echo "FAIL" || echo "OK"
```

- [ ] **Debug tooling absent from production images.**

---

## Application-layer

### `SAML_WANT_ASSERTIONS_SIGNED=true`

**Rationale.** Unsigned SAML assertions can be tampered with. Without signature verification, an attacker who can MITM the assertion can impersonate any user.

**Set.** This is the default in production; verify it is not overridden.

**Verify.**

```bash
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep SAML_WANT_ASSERTIONS_SIGNED
# SAML_WANT_ASSERTIONS_SIGNED=true
```

- [ ] **Signed assertions enforced.**

### Approval workflow enabled for high-risk operations

**Rationale.** A single operator should not be able to switch over a critical application without a second pair of eyes. See [Security Architecture §9](SECURITY_ARCHITECTURE.md).

**Set.** Per application, configure the approval policy in **Settings > Application > Approvals**:

| Risk | Operations | Approvers needed |
|------|------------|-----------------:|
| Low | diagnose, custom commands | 0 |
| Medium | start, stop, restart | configurable (0 or 1) |
| High | switchover, rebuild | 1 |
| Critical | DR commit, break-glass | 2 |

**Verify.**

```sql
SELECT name, settings->'approvals' AS approvals FROM applications;
```

- [ ] **Approval policy configured for production apps.**

### Break-glass accounts pre-provisioned

**Rationale.** When OIDC is down or all tokens have expired, you still need a way in. Pre-provisioned break-glass accounts with passwords sealed in an external vault meet this need.

**Set.** See [Security Architecture §10](SECURITY_ARCHITECTURE.md).

**Verify.**

```sql
SELECT username, is_active FROM break_glass_accounts;
-- Expect: at least 2 active accounts
```

- [ ] **Break-glass procedure tested annually.**

---

## Verification summary script

Run this against your production install at any time:

```bash
#!/usr/bin/env bash
# scripts/hardening-check.sh
set -e
NS=${NAMESPACE:-appcontrol}

echo "== JWT_SECRET length"
LEN=$(kubectl get secret -n $NS appcontrol-jwt -o jsonpath='{.data.jwt-secret}' | base64 -d | wc -c)
[[ $LEN -ge 32 ]] && echo "OK ($LEN bytes)" || echo "FAIL ($LEN bytes)"

echo "== APP_ENV"
ENV=$(kubectl exec -n $NS deploy/appcontrol-backend -- printenv APP_ENV)
[[ "$ENV" == "production" ]] && echo "OK" || echo "FAIL (got '$ENV')"

echo "== CORS_ORIGINS"
CORS=$(kubectl exec -n $NS deploy/appcontrol-backend -- printenv CORS_ORIGINS)
[[ -n "$CORS" ]] && echo "OK ($CORS)" || echo "FAIL"

echo "== TLS on gateway"
TLS=$(kubectl exec -n $NS deploy/appcontrol-gateway -- printenv TLS_ENABLED)
[[ "$TLS" == "true" ]] && echo "OK" || echo "FAIL"

echo "== NetworkPolicies"
COUNT=$(kubectl get networkpolicy -n $NS --no-headers | wc -l)
[[ $COUNT -ge 3 ]] && echo "OK ($COUNT policies)" || echo "FAIL ($COUNT policies)"

echo "== Backend non-root"
UID=$(kubectl get pod -n $NS -l app.kubernetes.io/component=backend -o jsonpath='{.items[0].spec.securityContext.runAsUser}')
[[ "$UID" != "0" && -n "$UID" ]] && echo "OK (UID=$UID)" || echo "FAIL"

echo "== Retention"
RET=$(kubectl exec -n $NS deploy/appcontrol-backend -- printenv RETENTION_ACTION_LOG_DAYS)
[[ -n "$RET" && "$RET" != "0" ]] && echo "OK (retention=$RET days)" || echo "FAIL"

echo "== JSON logs"
FMT=$(kubectl exec -n $NS deploy/appcontrol-backend -- printenv LOG_FORMAT)
[[ "$FMT" == "json" ]] && echo "OK" || echo "FAIL"
```

Schedule this in your CI / CronJob to alert on drift.

---

## Reference

- [Security Architecture](SECURITY_ARCHITECTURE.md) — design rationale for each control
- [Configuration](CONFIGURATION.md) — full env var reference
- [Compliance — DORA / NIS2](COMPLIANCE_DORA_NIS2.md) — regulatory mapping for each item
- [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md) — initial deployment steps
