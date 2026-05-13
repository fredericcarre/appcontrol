# Compliance — DORA / NIS2

This page maps AppControl's mechanisms to the regulatory requirements of:

- **DORA — Regulation (EU) 2022/2554**: Digital Operational Resilience Act. In force since **17 January 2025** for financial entities and their critical ICT third-party providers.
- **NIS2 — Directive (EU) 2022/2555**: Network and Information Security 2. Transposed by EU member states by **17 October 2024** for essential and important entities across critical sectors.

For each article we list the mechanism inside AppControl that produces the evidence, the table or log that captures it, and a verification query an auditor can run.

For the architectural design of the mechanisms, see [Security Architecture](SECURITY_ARCHITECTURE.md). For backup and retention of the evidence itself, see [Backup & Restore](BACKUP_RESTORE.md).

## Table of contents

- [DORA mapping](#dora-mapping)
- [NIS2 mapping (Article 21)](#nis2-mapping-article-21)
- [Exporting evidence for an auditor](#exporting-evidence-for-an-auditor)
- [Report endpoint reference](#report-endpoint-reference)
- [What AppControl does NOT cover](#what-appcontrol-does-not-cover)
- [Sanctions reference](#sanctions-reference)

---

## DORA mapping

DORA divides ICT risk management into five pillars. AppControl directly addresses Pillar I (Risk Management Framework — Articles 5–16) and Pillar III (Digital Operational Resilience Testing — Articles 24–27).

### Article-by-article mapping

| Article | Requirement (paraphrased) | AppControl mechanism | Evidence (table / log / file) | Verification query |
|---------|---------------------------|----------------------|-------------------------------|--------------------|
| **Art. 8(1)(a)** | Maintain a comprehensive ICT systems inventory | Applications and components are first-class entities with type, owner, agent, site | `applications`, `components` tables | `SELECT id, name, primary_site_id, created_at FROM applications WHERE deleted_at IS NULL;` |
| **Art. 8(2)** | Map dependencies between ICT assets supporting business functions | DAG of components with explicit dependency edges; visualized in Map View | `dependencies` table; UI Map View | `SELECT from_component_id, to_component_id FROM dependencies WHERE application_id = '<aid>';` |
| **Art. 8(3)** | Classify ICT assets by criticality | Application-level `criticality` tag; per-component `rebuild_protected` flag | `applications.tags`, `components.rebuild_protected` | `SELECT name, tags->>'criticality' FROM applications;` |
| **Art. 8(4)** | Update the inventory on any change | Every config change writes a versioned snapshot | `config_versions` (append-only, before/after JSONB) | `SELECT entity_id, change_type, created_at FROM config_versions ORDER BY created_at DESC LIMIT 20;` |
| **Art. 9(1)** | Cryptographic protection of data in transit / at rest | mTLS for agent ↔ gateway; TLS for everything else; encrypted backups (see [Hardening](HARDENING.md)) | TLS config in deployment manifests | `kubectl exec -n appcontrol deploy/appcontrol-gateway -- printenv TLS_ENABLED` → `true` |
| **Art. 9(4)** | Identity and access management with least-privilege | 5-level RBAC per application + workspaces (site scope) | `app_permissions_users`, `app_permissions_teams`, `workspace_sites`, `workspace_members` | `SELECT user_id, app_id, permission_level FROM app_permissions_users WHERE expires_at IS NULL OR expires_at > now();` |
| **Art. 11(1)** | Implement and maintain a business continuity policy | DR switchover engine with 6 phases and rollback; dry-run for plan validation | `switchover_log`, `applications.dr_site_id`, dry-run results | `SELECT application_id, phase, status, started_at FROM switchover_log ORDER BY started_at DESC LIMIT 20;` |
| **Art. 11(2)** | Test the continuity plan at least annually | Switchover dry-run and drill, chronologically tracked | `action_log` rows with `action_type IN ('switchover','switchover_dry_run')` | `SELECT count(*) FROM action_log WHERE action_type LIKE 'switchover%' AND created_at > now() - interval '1 year';` |
| **Art. 12(1)** | Procedures for the reconstruction of ICT systems after a destructive event | 3-level diagnostic + targeted rebuild engine (`rebuild_cmd`, `rebuild_infra_cmd`, bastion) | `action_log` rows with `action_type = 'rebuild'`; `components.rebuild_*` columns | `SELECT id, name, rebuild_cmd, rebuild_protected FROM components WHERE rebuild_cmd IS NOT NULL;` |
| **Art. 12(3)** | Document the order of recovery and RTO/RPO | DAG order is recorded; per-rebuild RTR (Recovery Time for Rebuild) is measured | `action_log.details.rtr_seconds` | `SELECT target_id, details->>'rtr_seconds' AS rtr FROM action_log WHERE action_type = 'rebuild' AND status = 'completed';` |
| **Art. 16(1)** | Record every incident with timestamp, actor, action, outcome | Append-only `action_log` written before every action runs; `state_transitions` records every FSM change | `action_log`, `state_transitions` | `SELECT user_id, action_type, target_id, status, created_at FROM action_log WHERE created_at > '<incident_start>' ORDER BY created_at;` |
| **Art. 16(2)** | Retain incident records for a sufficient duration | Configurable retention; recommended **5 years minimum** | `RETENTION_ACTION_LOG_DAYS` env var | `kubectl exec -n appcontrol deploy/appcontrol-backend -- printenv RETENTION_ACTION_LOG_DAYS` |
| **Art. 17(1)** | Notify competent authorities of major incidents | Manual export of `action_log` + incidents report via API | `/reports/audit`, `/reports/incidents` | `curl /api/v1/reports/audit?from=<t1>&to=<t2>` |
| **Art. 24** | Maintain a digital operational resilience testing programme | Drill records and dry-run history | `switchover_log` rows with `mode IN ('DRY_RUN','SELECTIVE','PROGRESSIVE')`; `action_log.dry_run = true` | `SELECT application_id, mode, started_at, ended_at FROM switchover_log WHERE mode != 'FULL';` |
| **Art. 25** | Test scenarios for cyber-attacks and data corruption | DR drill in supervision mode; corruption-detection workflow | `switchover_log` with `selective` mode + diagnostic Level 2 (integrity) results | `SELECT component_id, exit_code FROM check_events WHERE check_type = 'integrity' AND created_at > now() - interval '90 days';` |
| **Art. 27** | Report on the outcomes of resilience testing | `/reports/compliance` and `/reports/rto` endpoints | `report_compliance.pdf`, `report_rto.pdf` | `curl /api/v1/apps/<id>/reports/compliance` |

### Append-only guarantee

DORA Article 16 requires incident records that cannot be tampered with after the fact. AppControl enforces this at the schema level:

- `action_log`, `state_transitions`, `check_events`, `switchover_log`, `config_versions` are append-only by design.
- `UPDATE` and `DELETE` are not used by any backend code path against these tables (CI lint rule).
- Retention is implemented as **partition drop** (`check_events`) or **row archive** (`action_log` → `action_log_archive`), never as in-place delete.

Inspect:

```sql
-- These tables have a CHECK / trigger or simply no UPDATE/DELETE in code paths
\dt action_log
\dt state_transitions
\dt switchover_log
\dt config_versions
```

For DORA Article 12 (reconstruction), the **`config_versions` table stores both the before and after** of every configuration change as JSONB. Auditors can reconstruct what the system looked like at any point in time:

```sql
-- What did application X look like on date Y?
SELECT after FROM config_versions
WHERE entity_id = '<aid>' AND created_at <= '2025-06-01'
ORDER BY created_at DESC LIMIT 1;
```

---

## NIS2 mapping (Article 21)

NIS2 Article 21 lists 10 risk-management measures. The table below shows which AppControl mechanism supports each measure.

| Measure (Art. 21.2) | AppControl support | How |
|---------------------|--------------------|-----|
| **(a)** Policies on risk analysis and information system security | Application-level mode (advisory / operate / drill / DR) — explicit choice of autonomy level | UI `applications.settings.mode`; documented in [README](https://github.com/fredericcarre/appcontrol/blob/main/README.md#garde-fous-par-conception) |
| **(b)** Incident handling | FSM + append-only audit; 3-level diagnostic; rebuild engine | `action_log`, `state_transitions`, diagnostic endpoints; see [DORA Art. 16](#article-by-article-mapping) |
| **(c)** Business continuity (backup, DR, crisis management) | DR switchover (6 phases), dry-run, drill mode | `switchover_log`; see [Disaster Recovery](DISASTER_RECOVERY.md) and [DORA Art. 11–12](#article-by-article-mapping) |
| **(d)** Supply chain security | mTLS-only ingest of agents; signed binary updates; enrollment audit | `pki_authorities`, `enrollment_tokens`, signed agent bundles; see [Security Architecture §3, §12](SECURITY_ARCHITECTURE.md) |
| **(e)** Security in network and information systems acquisition, development, and maintenance | SBOM published with every release; Trivy scan in CI; semver-versioned upgrades | `release-assets/sbom.cdx.json`; [UPGRADE.md](UPGRADE.md) |
| **(f)** Policies and procedures to assess the effectiveness of cybersecurity measures | Compliance and RTO reports; drill records; hardening checklist | `/reports/compliance`; [HARDENING.md](HARDENING.md) |
| **(g)** Basic cyber hygiene practices and cybersecurity training | Hardening checklist; operator role separation | [HARDENING.md](HARDENING.md) |
| **(h)** Policies and procedures regarding the use of cryptography and encryption | mTLS, TLS 1.3, scram-sha-256 for DB; encrypted backups | TLS config; [BACKUP_RESTORE.md — Encryption](BACKUP_RESTORE.md#encryption-at-rest) |
| **(i)** Human-resources security, access control policies and asset management | RBAC, workspaces, SAML group sync, share links with expiry | `app_permissions_users/teams`, `workspace_*`, `saml_group_mappings`, `app_share_links` |
| **(j)** Multi-factor authentication or continuous authentication solutions | **MFA depends on the upstream IdP.** AppControl uses OIDC / SAML — enforce MFA in your IdP (Keycloak, Okta, Azure AD). Local-auth flows do not enforce MFA. | OIDC / SAML configuration on the IdP side |

!!! warning "MFA caveat"
    AppControl does **not** itself implement MFA. Multi-factor authentication is enforced at the IdP layer (OIDC / SAML). Disable local-auth in production (set `APP_ENV=production`) so MFA cannot be bypassed.

---

## Exporting evidence for an auditor

When an auditor (ACPR, EBA, national NIS2 competent authority, internal compliance) requests evidence, produce a report bundle using the endpoints below. Every report supports a configurable date range.

### Audit trail (Art. 16)

```bash
# Full audit trail for an app over a given period
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/audit?from=2025-01-01&to=2025-12-31" \
  -H "Authorization: Bearer $TOKEN" \
  -o audit-2025.json

# Or organization-wide
curl "https://appcontrol.example.com/api/v1/reports/audit?from=2025-01-01&to=2025-12-31" \
  -H "Authorization: Bearer $TOKEN" \
  -o audit-org-2025.json
```

The JSON payload contains:

- `actions[]`: every user-initiated action (start, stop, switchover, edit, etc.)
- `state_transitions[]`: every component state change
- `enrollment_events[]`: every agent enrollment
- `break_glass_sessions[]`: every emergency-access session
- For each entry: timestamp, actor (user_id + email), target, result, and a stable hash for chain-of-custody

### Compliance report (DORA Article 27)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/compliance?year=2025" \
  -H "Authorization: Bearer $TOKEN" \
  -o compliance-2025.json
```

Includes:

- Total drills executed (dry-runs + full switchovers)
- Average and worst-case RTO
- Number of incidents (FSM transitions to `FAILED` or `UNREACHABLE`)
- Rebuild events with RTR (Recovery Time for Rebuild)

### Incidents and DRP report (Art. 17 + Art. 11)

```bash
# Incidents
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/incidents?from=2025-01-01" \
  -H "Authorization: Bearer $TOKEN" -o incidents-2025.json

# Disaster Recovery Plan execution history
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/pra" \
  -H "Authorization: Bearer $TOKEN" -o pra-history.json
```

### Switchovers report (Art. 11 + Art. 25)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/pra" \
  -H "Authorization: Bearer $TOKEN" -o switchovers.json
```

The `pra` report (Plan de Reprise d'Activité — French acronym for DRP) lists every switchover, dry-run, and drill with phase-by-phase timing.

### RTO report (Art. 12)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/rto?year=2025" \
  -H "Authorization: Bearer $TOKEN" -o rto-2025.json
```

Provides actual vs configured RTO per application, with deviation alerts when a switchover exceeded the configured target.

### MTTR (Mean Time To Recovery)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/mttr?from=2025-01-01" \
  -H "Authorization: Bearer $TOKEN" -o mttr-2025.json
```

Calculated as the average time between FAILED → RUNNING transitions for an application's components, excluding planned operations.

### Activity feed (live audit stream)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/activity?since=2025-12-31T00:00:00Z" \
  -H "Authorization: Bearer $TOKEN" -o activity.json
```

Useful for real-time SIEM ingestion.

### PDF bundle (one file per app, for the auditor)

```bash
curl "https://appcontrol.example.com/api/v1/apps/<app_id>/reports/export?period=2025" \
  -H "Authorization: Bearer $TOKEN" \
  -o appcontrol-evidence-2025.pdf
```

The PDF aggregates all of the above into a single signed file with:

- Cover page (org name, period, generated-at timestamp)
- DORA article cross-reference for each section
- Audit trail (chronological)
- Incident timeline
- Switchover history with RTO/RPO
- Rebuild history with RTR
- Configuration version snapshots (before/after for every material change)
- Signature footer (SHA-256 of the bundle; cross-references the action log)

The PDF is self-contained — an auditor can reconcile it against the JSON exports above.

### Format reference

| Endpoint | Format | Default range | Notes |
|----------|--------|---------------|-------|
| `/audit` | JSON | last 30 days | Pass `?format=csv` for spreadsheet ingest |
| `/incidents` | JSON | last 30 days | Includes root-cause if labeled |
| `/pra` | JSON | last 12 months | Phase-by-phase switchover detail |
| `/compliance` | JSON | last year | Aggregate KPIs |
| `/rto` | JSON | last year | per-app RTO actual vs target |
| `/mttr` | JSON | last 30 days | per-app MTTR |
| `/export` | PDF | period parameter | Full signed bundle |

---

## Report endpoint reference

Defined in `crates/backend/src/api/reports/`. All endpoints require `JWT` auth and at least `view` permission on the target app.

| Endpoint | File | Purpose |
|----------|------|---------|
| `GET /apps/:app_id/reports/audit` | `audit.rs` | Full action_log + state_transitions for an app |
| `GET /reports/audit` | `audit.rs` | Org-wide audit (admin only) |
| `GET /apps/:app_id/activity` | `audit.rs` | Live activity feed |
| `GET /apps/:app_id/reports/availability` | `availability.rs` | Uptime / downtime per component |
| `GET /apps/:app_id/health-summary` | `availability.rs` | Component-level health rollup |
| `GET /apps/:app_id/reports/compliance` | `dora.rs` | Aggregated DORA compliance KPIs |
| `GET /apps/:app_id/reports/rto` | `dora.rs` | RTO analysis |
| `GET /apps/:app_id/reports/mttr` | `dora.rs` | MTTR analysis |
| `GET /apps/:app_id/reports/pra` | `incidents.rs` | DRP / switchover history |
| `GET /apps/:app_id/reports/incidents` | `incidents.rs` | Failure incidents |
| `GET /apps/:app_id/reports/export` | `export.rs` | PDF bundle |

All endpoints accept `from` / `to` query parameters as ISO 8601 timestamps.

---

## What AppControl does NOT cover

Honesty matters for compliance. AppControl is one tool in a regulated stack — these items require **other** systems.

| Requirement | Why AppControl does not cover it | What does |
|-------------|----------------------------------|-----------|
| **Penetration testing** (DORA Art. 26, TLPT) | Out of scope — AppControl is the target, not the tester | Third-party Red Team or TIBER-EU-aligned vendor |
| **Third-party SBOM tracking for all dependencies** | AppControl publishes its own SBOM (CycloneDX) but does not catalog the SBOMs of every system it monitors | Snyk, Dependency-Track, GitHub Advanced Security |
| **Business continuity beyond IT** (HR, premises, communications) | DORA Art. 11 covers ICT continuity; broader BCM requires more | BCM tool (ServiceNow BCM, Fusion Framework) |
| **Vulnerability scanning of monitored hosts** | AppControl monitors application state, not OS-level CVEs | Qualys, Tenable, Wiz, native cloud security center |
| **Network IDS / IPS** (NIS2 21(2)(b)) | AppControl has rate limiting and mTLS auth but is not an IDS | Suricata, Snort, Zscaler, cloud-native WAF |
| **Encryption key management** (DORA Art. 9, NIS2 21(2)(h)) | AppControl uses keys but does not provide HSM functions | HashiCorp Vault, AWS KMS, Azure Key Vault |
| **Centralized IAM with MFA** | AppControl delegates auth to OIDC / SAML — your IdP enforces MFA | Keycloak, Okta, Azure AD, ADFS |
| **Real-time SIEM correlation** | AppControl emits JSON logs and audit data; correlation is downstream | Splunk, Elastic SIEM, Datadog, Sentinel |
| **Application-layer DLP** | AppControl does not inspect business data | Microsoft Purview, Symantec DLP |
| **Outsourcing register** (DORA Art. 28–30) | AppControl tracks ICT inventory, not third-party contracts | GRC tool (Archer, ServiceNow GRC) |
| **Quantitative ICT risk assessment** (DORA Art. 6) | AppControl provides metrics (RTO, MTTR, RTR) — quantification of *business* risk is your CFO's job | Risk-management tool fed by AppControl exports |

AppControl is the **operational evidence layer**. It supplies the data (DAG, audit, state transitions, RTO measurements) that the items above consume.

---

## Sanctions reference

For context, both regulations have material penalties:

### DORA

- Up to **2 % of the entity's annual global turnover** for the firm.
- Up to **€1 million** for individual senior managers, applied personally.
- Plus reputational sanctions: public naming, business restrictions.

Source: Regulation (EU) 2022/2554, Article 50.

### NIS2

- Essential entities: up to **€10 million or 2 % of global turnover** (whichever is higher).
- Important entities: up to **€7 million or 1.4 % of global turnover**.
- Personal liability for executives in case of gross negligence.

Source: Directive (EU) 2022/2555, Articles 34–35.

These thresholds are the rationale for treating audit retention and reconstruction capability as **mandatory** rather than nice-to-have.

---

## Reference

- [Security Architecture](SECURITY_ARCHITECTURE.md) — implementation details of each mechanism
- [Hardening](HARDENING.md) — operational hardening checklist
- [Backup & Restore](BACKUP_RESTORE.md) — retention and off-site evidence storage
- [Disaster Recovery](DISASTER_RECOVERY.md) — DR drill procedure for Art. 11
- [Upgrade](UPGRADE.md) — patch-management evidence for NIS2 21(2)(e)

External:

- [Regulation (EU) 2022/2554 (DORA)](https://eur-lex.europa.eu/eli/reg/2022/2554/oj)
- [Directive (EU) 2022/2555 (NIS2)](https://eur-lex.europa.eu/eli/dir/2022/2555/oj)
- [ENISA NIS2 guidance](https://www.enisa.europa.eu/topics/nis-directive)
- [EBA / ESMA / EIOPA DORA technical standards](https://www.eba.europa.eu/regulation-and-policy/operational-resilience)
