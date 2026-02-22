# Security & Resilience Architecture — AppControl v4

> **Document Status:** Living specification — updated with each security iteration.
> **Last Updated:** 2026-02-22
> **Audience:** Production Directors, Security Officers, Architects

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Threat Model](#2-threat-model)
3. [Agent Identity & Trust Chain](#3-agent-identity--trust-chain)
4. [Message Reliability Protocol](#4-message-reliability-protocol)
5. [Multi-Gateway Failover](#5-multi-gateway-failover)
6. [Rate Limiting & DoS Protection](#6-rate-limiting--dos-protection)
7. [WebSocket Security](#7-websocket-security)
8. [Process Execution Security](#8-process-execution-security)
9. [Approval Workflows (4-Eyes Principle)](#9-approval-workflows-4-eyes-principle)
10. [Break-Glass Emergency Access](#10-break-glass-emergency-access)
11. [Credential Vault Integration](#11-credential-vault-integration)
12. [Centralized Agent Update](#12-centralized-agent-update)
13. [Certificate Lifecycle Management](#13-certificate-lifecycle-management)
14. [Configuration Security](#14-configuration-security)
15. [Competitive Benchmarking](#15-competitive-benchmarking)

---

## 1. Executive Summary

This document describes the security and resilience architecture for AppControl v4, covering 15 domains from agent identity to certificate lifecycle. Each section includes:

- **Threat description** — what can go wrong
- **Architecture diagram** — how the solution works
- **Implementation details** — what was built
- **Competitive reference** — which industry product inspired the approach

### Design Principles

1. **Defense in depth** — no single control prevents all attacks
2. **Secure by default** — production deployment requires explicit security configuration
3. **Don't restrict user commands** — secure the execution context, not the content
4. **Audit everything** — every action is traceable, every transition is logged
5. **Fail closed** — when in doubt, deny access

---

## 2. Threat Model

```
┌─────────────────────────────────────────────────────────────────┐
│                        THREAT LANDSCAPE                         │
├─────────────┬───────────────────────────────────────────────────┤
│ T1          │ Agent Impersonation                               │
│             │ Attacker with valid TLS cert claims another       │
│             │ agent_id → receives commands for critical servers  │
├─────────────┼───────────────────────────────────────────────────┤
│ T2          │ Message Loss                                      │
│             │ Backend crash during CommandResult → lost state    │
│             │ Agent never knows result was not acknowledged      │
├─────────────┼───────────────────────────────────────────────────┤
│ T3          │ Gateway SPOF                                      │
│             │ Single gateway failure → all zone agents offline   │
│             │ No commands can be sent, no state updates          │
├─────────────┼───────────────────────────────────────────────────┤
│ T4          │ Unauthorized WebSocket Snooping                   │
│             │ Authenticated user subscribes to apps they have   │
│             │ no permission for → information disclosure         │
├─────────────┼───────────────────────────────────────────────────┤
│ T5          │ Process Orphaning                                 │
│             │ Agent crash kills start_cmd process → partial     │
│             │ startup, corrupted application state               │
├─────────────┼───────────────────────────────────────────────────┤
│ T6          │ Runaway Child Process                             │
│             │ check_cmd infinite loop / memory bomb → host      │
│             │ exhaustion, impacts all components on server       │
├─────────────┼───────────────────────────────────────────────────┤
│ T7          │ Unauthorized Critical Operations                  │
│             │ Single operator starts DR switchover without       │
│             │ review → production outage                         │
├─────────────┼───────────────────────────────────────────────────┤
│ T8          │ Emergency Lockout                                 │
│             │ OIDC provider down + all tokens expired → nobody  │
│             │ can access the platform during an incident         │
├─────────────┼───────────────────────────────────────────────────┤
│ T9          │ Default Credentials in Production                 │
│             │ JWT secret left as "dev-secret-change-in-prod"    │
│             │ → any attacker can forge valid tokens              │
├─────────────┼───────────────────────────────────────────────────┤
│ T10         │ Stale Agent Binaries                              │
│             │ Agents running old versions with known vulns      │
│             │ No way to detect or update remotely                │
├─────────────┼───────────────────────────────────────────────────┤
│ T11         │ Credential Exposure in Commands                   │
│             │ Database passwords embedded in start_cmd strings  │
│             │ Visible in logs, audit trail, API responses        │
├─────────────┼───────────────────────────────────────────────────┤
│ T12         │ API Abuse / DoS                                   │
│             │ No rate limiting → brute force auth endpoints     │
│             │ or spam start/stop operations                      │
└─────────────┴───────────────────────────────────────────────────┘
```

---

## 3. Agent Identity & Trust Chain

**Threat addressed:** T1 (Agent Impersonation)
**Inspired by:** IBM BigFix (cryptographic action signing), HashiCorp Consul (mTLS identity binding)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                     AGENT IDENTITY CHAIN                         │
│                                                                  │
│  ┌─────────┐    mTLS     ┌─────────┐    mTLS    ┌─────────────┐│
│  │  Agent   │◄───────────►│ Gateway │◄──────────►│   Backend   ││
│  │          │ cert CN=    │         │ extracts   │             ││
│  │          │ <agent-id>  │         │ cert_fp +  │ validates   ││
│  └─────────┘             │         │ CN from    │ agent_id == ││
│                          │         │ TLS layer  │ cert_cn     ││
│                          └─────────┘            └─────────────┘│
│                                                                  │
│  Validation Flow:                                                │
│  1. Agent connects with TLS client certificate                   │
│  2. Gateway extracts CN and SHA-256 fingerprint from cert        │
│  3. Agent sends Register { agent_id, hostname, ... }             │
│  4. Gateway verifies: agent_id == UUID derived from cert CN      │
│  5. Gateway forwards to backend with cert_fingerprint            │
│  6. Backend stores fingerprint, rejects mismatches on reconnect  │
│                                                                  │
│  On mismatch → connection rejected + security alert in action_log│
└──────────────────────────────────────────────────────────────────┘
```

### Implementation

- **Register message** extended with `cert_fingerprint: Option<String>` for the gateway to forward
- **Gateway** extracts CN/SAN from TLS handshake and validates against claimed `agent_id`
- **Backend** stores `certificate_fingerprint` in `agents` table, rejects changes unless admin-approved
- **Backward compatible:** agents without TLS certs are accepted but flagged as `identity_verified: false`

---

## 4. Message Reliability Protocol

**Threat addressed:** T2 (Message Loss)
**Inspired by:** IBM BigFix (sequence numbers + ack/nak)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│               MESSAGE ACKNOWLEDGMENT PROTOCOL                    │
│                                                                  │
│   Agent                    Gateway                Backend        │
│     │                        │                      │            │
│     │──CommandResult(seq=42)─►│──────forward────────►│            │
│     │                        │                      │            │
│     │                        │                      │──persist──►│
│     │                        │                      │  to DB     │
│     │                        │◄─────Ack(seq=42)─────│            │
│     │◄───Ack(seq=42)────────│                      │            │
│     │                        │                      │            │
│     │   [if no Ack in 30s]   │                      │            │
│     │──CommandResult(seq=42)─►│   (retransmit)       │            │
│     │                        │                      │            │
│                                                                  │
│   Messages with sequence_id:                                     │
│   • CommandResult (agent → backend) — critical results           │
│   • CheckResult (agent → backend) — state-changing results       │
│   • ExecuteCommand (backend → agent) — command delivery          │
│                                                                  │
│   Messages WITHOUT sequence_id (fire-and-forget):                │
│   • Heartbeat — loss is tolerable (next one in 30s)              │
│   • UpdateConfig — idempotent (full snapshot replaces state)     │
└──────────────────────────────────────────────────────────────────┘
```

### Implementation

- `sequence_id: Option<u64>` added to `AgentMessage::CommandResult` and `AgentMessage::CheckResult`
- Agent maintains monotonic counter per connection, resets on reconnect
- Backend sends `Ack { sequence_id }` after successful DB persistence
- Agent retransmits un-acked messages after 30s timeout (max 3 retries)
- Duplicate detection: backend deduplicates by `(agent_id, sequence_id)` pair

---

## 5. Multi-Gateway Failover

**Threat addressed:** T3 (Gateway SPOF)
**Inspired by:** HashiCorp Consul (multi-server with failover), ServiceNow (MID Server tiers)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                   MULTI-GATEWAY FAILOVER                         │
│                                                                  │
│                    ┌──────────────┐                               │
│                    │   Backend    │                               │
│                    │   Cluster    │                               │
│                    └──┬───────┬──┘                               │
│                       │       │                                   │
│              ┌────────┘       └────────┐                         │
│              ▼                         ▼                         │
│    ┌──────────────┐          ┌──────────────┐                    │
│    │ Gateway PRD-1│          │ Gateway PRD-2│                    │
│    │  (primary)   │          │  (secondary) │                    │
│    └──────┬───────┘          └──────┬───────┘                    │
│           │                         │                            │
│           ▼                         ▼                            │
│    ┌─────────────────────────────────────────┐                   │
│    │               Agent                      │                  │
│    │                                          │                  │
│    │  gateway_urls:                           │                  │
│    │    - wss://gw-prd-01:443  (primary)     │                  │
│    │    - wss://gw-prd-02:443  (secondary)   │                  │
│    │    - wss://gw-dr:443      (backup)      │                  │
│    │                                          │                  │
│    │  Failover: ordered, backoff, auto-reset  │                  │
│    └─────────────────────────────────────────┘                   │
│                                                                  │
│  Behavior:                                                       │
│  1. Agent connects to gateway_urls[0] (primary)                  │
│  2. On failure → try gateway_urls[1] with backoff                │
│  3. On failure → try gateway_urls[2] with backoff                │
│  4. Cycle through all URLs with exponential backoff              │
│  5. Periodically try to return to primary (every 5 min)          │
│  6. Backward compat: single "url" field still works              │
└──────────────────────────────────────────────────────────────────┘
```

### Agent Configuration

```yaml
# New multi-gateway config (recommended)
gateway:
  urls:
    - wss://gateway-prd-01.company.com:443
    - wss://gateway-prd-02.company.com:443
    - wss://gateway-dr.company.com:443
  failover_strategy: ordered   # ordered | round-robin
  primary_retry_secs: 300      # try to return to primary every 5 min
  reconnect_interval_secs: 10

# Legacy single-gateway config (still supported)
gateway:
  url: wss://gateway-prd.company.com:443
```

---

## 6. Rate Limiting & DoS Protection

**Threat addressed:** T12 (API Abuse / DoS)
**Inspired by:** Industry standard (all enterprise platforms)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                     RATE LIMITING LAYERS                          │
│                                                                  │
│  Layer 1: Per-IP (auth endpoints)                                │
│  ┌────────────────────────────────────────┐                      │
│  │ /auth/oidc/callback  → 10 req/min/IP  │                      │
│  │ /auth/saml/acs       → 10 req/min/IP  │                      │
│  │ /api-keys            → 10 req/min/IP  │                      │
│  └────────────────────────────────────────┘                      │
│                                                                  │
│  Layer 2: Per-User (operations)                                  │
│  ┌────────────────────────────────────────┐                      │
│  │ POST /apps/:id/start     → 5/min/user │                      │
│  │ POST /apps/:id/stop      → 5/min/user │                      │
│  │ POST /apps/:id/switchover → 2/min/user│                      │
│  │ POST /apps/:id/rebuild   → 2/min/user │                      │
│  └────────────────────────────────────────┘                      │
│                                                                  │
│  Layer 3: Per-User (reads)                                       │
│  ┌────────────────────────────────────────┐                      │
│  │ GET /* (all read endpoints) → 200/min  │                      │
│  └────────────────────────────────────────┘                      │
│                                                                  │
│  Implementation: tower-governor middleware on Axum router         │
│  Response on limit: HTTP 429 Too Many Requests                   │
│  Headers: X-RateLimit-Remaining, X-RateLimit-Reset               │
└──────────────────────────────────────────────────────────────────┘
```

---

## 7. WebSocket Security

**Threat addressed:** T4 (Unauthorized WebSocket Snooping)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│               WEBSOCKET PERMISSION-CHECKED SUBSCRIBE             │
│                                                                  │
│  Frontend          Backend WebSocket Hub           Database      │
│     │                      │                          │          │
│     │─Subscribe(app_id)───►│                          │          │
│     │                      │──effective_permission()─►│          │
│     │                      │◄──PermissionLevel::View──│          │
│     │                      │                          │          │
│     │                      │  if perm >= View:        │          │
│     │◄─Events for app_id───│    add subscription      │          │
│     │                      │  else:                   │          │
│     │◄─PermissionDenied────│    reject + log          │          │
│     │                      │                          │          │
└──────────────────────────────────────────────────────────────────┘
```

---

## 8. Process Execution Security

**Threat addressed:** T5 (Process Orphaning), T6 (Runaway Child Process)

### Design Philosophy: Secure the Context, Not the Content

AppControl executes user-defined shell commands (`check_cmd`, `start_cmd`, `stop_cmd`, etc.). These commands are written by operations teams and legitimately contain pipes, redirections, variable substitutions, and complex shell constructs. **We do NOT filter or restrict command content.**

Instead, we secure:
- **WHO** can define commands → RBAC `edit` permission required
- **HOW** commands are executed → detached processes, resource limits
- **WHAT** happens after → full audit trail, state machine transitions

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                  PROCESS EXECUTION MODEL                         │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    AGENT PROCESS                             │ │
│  │                                                             │ │
│  │  Received: ExecuteCommand { command: "...", timeout: 120 }  │ │
│  │                                                             │ │
│  │  Routing logic:                                             │ │
│  │  ┌──────────────────────┬──────────────────────────────┐    │ │
│  │  │ Command Type         │ Execution Mode               │    │ │
│  │  ├──────────────────────┼──────────────────────────────┤    │ │
│  │  │ check_cmd            │ Sync (wait for result)       │    │ │
│  │  │ integrity_check_cmd  │ Sync (wait for result)       │    │ │
│  │  │ infra_check_cmd      │ Sync (wait for result)       │    │ │
│  │  │ start_cmd            │ Async DETACHED (double-fork) │    │ │
│  │  │ stop_cmd             │ Async DETACHED (double-fork) │    │ │
│  │  │ rebuild_cmd          │ Async DETACHED (double-fork) │    │ │
│  │  │ custom_commands      │ Sync (wait for result)       │    │ │
│  │  └──────────────────────┴──────────────────────────────┘    │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  Detached Process (double-fork + setsid):                        │
│  ┌──────────┐                                                    │
│  │  Agent   │──fork()──►┌──────────┐──fork()──►┌──────────────┐ │
│  │ (parent) │           │ Child    │           │ Grandchild   │ │
│  │          │           │ setsid() │           │ • /dev/null  │ │
│  │ waitpid()│◄──exit()──│ exit(0)  │           │ • prlimit()  │ │
│  │          │           └──────────┘           │ • exec(cmd)  │ │
│  └──────────┘                                  │              │ │
│                                                │ Survives     │ │
│  Agent crash ──────────────────────────────── │ agent crash! │ │
│                                                └──────────────┘ │
│                                                                  │
│  Resource Limits (applied before exec):                          │
│  ┌────────────────────────────────────────┐                      │
│  │ RLIMIT_CPU     = 30 seconds           │                      │
│  │ RLIMIT_AS      = 512 MB               │                      │
│  │ RLIMIT_NOFILE  = 512 file descriptors │                      │
│  │ RLIMIT_NPROC   = 64 child processes   │                      │
│  │ Timeout        = configurable (120s)  │                      │
│  └────────────────────────────────────────┘                      │
└──────────────────────────────────────────────────────────────────┘
```

---

## 9. Approval Workflows (4-Eyes Principle)

**Threat addressed:** T7 (Unauthorized Critical Operations)
**Inspired by:** ServiceNow (requester != approver, CAB), Automic (segregation of duties)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                   APPROVAL WORKFLOW                               │
│                                                                  │
│  Operator A                Backend               Operator B      │
│     │                        │                      │            │
│     │──POST /apps/X/start───►│                      │            │
│     │  (high-risk op)        │                      │            │
│     │                        │──check risk level────►            │
│     │                        │                      │            │
│     │◄─202 Accepted──────────│                      │            │
│     │  approval_request_id   │                      │            │
│     │                        │──notify approvers────►│            │
│     │                        │  (WebSocket + email)  │            │
│     │                        │                      │            │
│     │                        │◄─POST /approvals/Y/  │            │
│     │                        │   approve             │            │
│     │                        │                      │            │
│     │                        │──validate:            │            │
│     │                        │  requester != approver│            │
│     │                        │  approver has perm    │            │
│     │                        │                      │            │
│     │                        │──execute operation────►            │
│     │◄─WS: OperationStarted─│                      │            │
│                                                                  │
│  Risk Classification:                                            │
│  ┌────────────────┬────────────────────────────────────────┐     │
│  │ Risk Level     │ Operations              │ Approval     │     │
│  ├────────────────┼─────────────────────────┼──────────────┤     │
│  │ Low            │ diagnose, check         │ None         │     │
│  │ Medium         │ start, stop, restart    │ Configurable │     │
│  │ High           │ switchover, rebuild     │ Required     │     │
│  │ Critical       │ break-glass, DR commit  │ 2 approvers  │     │
│  └────────────────┴─────────────────────────┴──────────────┘     │
│                                                                  │
│  Timeout: configurable per operation (default 15 min)            │
│  Auto-reject: on timeout, request moves to "expired"             │
│  Audit: full trail — who requested, who approved, when           │
└──────────────────────────────────────────────────────────────────┘
```

---

## 10. Break-Glass Emergency Access

**Threat addressed:** T8 (Emergency Lockout)
**Inspired by:** ServiceNow (Emergency Change + ECAB), Banking industry (Shamir secret sharing)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                   BREAK-GLASS PROCEDURE                          │
│                                                                  │
│  Normal Operations:                                              │
│  ┌──────────┐    OIDC/SAML    ┌──────────┐                      │
│  │ Operator │───────────────►│ Backend  │   Standard auth       │
│  └──────────┘                └──────────┘                       │
│                                                                  │
│  Emergency (OIDC down / all tokens expired):                     │
│  ┌──────────┐                ┌──────────┐                        │
│  │ On-call  │──break-glass──►│ Backend  │                        │
│  │ engineer │  account       │          │                        │
│  └──────────┘                └──────────┘                        │
│                                                                  │
│  Break-Glass Lifecycle:                                          │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │ 1. Pre-provisioned accounts: breakglass-01, breakglass-02│    │
│  │ 2. Passwords stored in external vault (sealed)           │    │
│  │ 3. Access request triggers:                              │    │
│  │    a) Immediate alert to ALL org admins (email+WS)       │    │
│  │    b) Session timer starts (default: 60 minutes)         │    │
│  │    c) All actions tagged: { break_glass: true }          │    │
│  │ 4. Session auto-expires after timeout                    │    │
│  │ 5. Post-incident:                                        │    │
│  │    a) Password rotated automatically                     │    │
│  │    b) Mandatory review of all break-glass actions        │    │
│  │    c) Incident report generated from action_log          │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Database Schema:                                                │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │ break_glass_accounts                                      │    │
│  │   id, username, password_hash, org_id, is_active          │    │
│  │                                                           │    │
│  │ break_glass_sessions (APPEND-ONLY)                        │    │
│  │   id, account_id, activated_by_ip, reason,                │    │
│  │   started_at, expires_at, ended_at, actions_taken         │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

---

## 11. Credential Vault Integration

**Threat addressed:** T11 (Credential Exposure in Commands)
**Inspired by:** Ansible ("Use but don't See"), BMC BladeLogic (AES-256-GCM write-only vault)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│              CREDENTIAL VAULT INTEGRATION                        │
│                                                                  │
│  Principle: "Use but don't See"                                  │
│  Operators can use credentials in commands without ever seeing   │
│  the plaintext secret value.                                     │
│                                                                  │
│  ┌──────────┐   ┌──────────┐   ┌───────────┐   ┌─────────────┐ │
│  │ Frontend │   │ Backend  │   │   Agent   │   │ Vault/KMS   │ │
│  │          │   │          │   │           │   │             │ │
│  │ $(secret │   │ resolves │   │ fetches   │   │ stores      │ │
│  │  :name)  │──►│ variable │──►│ from vault│──►│ encrypted   │ │
│  │ in UI    │   │ reference│   │ at runtime│   │ secrets     │ │
│  └──────────┘   └──────────┘   └───────────┘   └─────────────┘ │
│                                                                  │
│  Flow:                                                           │
│  1. Admin creates secret variable: name="ORACLE_PWD", value=*** │
│  2. Command references it: check_cmd="sqlplus user/$(secret:    │
│     ORACLE_PWD)@PROD"                                            │
│  3. Backend sends command with $(secret:ORACLE_PWD) placeholder  │
│  4. Agent resolves placeholder:                                  │
│     a) From local env var (APPCONTROL_SECRET_ORACLE_PWD)         │
│     b) From Vault (vault://secret/data/appcontrol/ORACLE_PWD)   │
│     c) From built-in encrypted store (fallback)                  │
│  5. Secret is substituted in memory, never written to disk/logs  │
│  6. After execution, memory is zeroed                            │
│                                                                  │
│  Vault Backends:                                                 │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Provider     │ Auth Method       │ Secret Path             │  │
│  ├──────────────┼───────────────────┼─────────────────────────┤  │
│  │ HashiCorp    │ AppRole / TLS     │ secret/data/appcontrol/ │  │
│  │ AWS KMS      │ IAM Role          │ /appcontrol/secrets/    │  │
│  │ Azure KV     │ Managed Identity  │ appcontrol-vault/       │  │
│  │ Built-in     │ Agent mTLS cert   │ Local AES-256 store     │  │
│  └──────────────┴───────────────────┴─────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 12. Centralized Agent Update

**Threat addressed:** T10 (Stale Agent Binaries)
**Inspired by:** Broadcom Automic (Centralized Agent Upgrade), IBM BigFix (relay-based distribution)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                 CENTRALIZED AGENT UPDATE                         │
│                                                                  │
│  Admin                  Backend               Agent              │
│    │                      │                     │                │
│    │──POST /admin/agent-  │                     │                │
│    │  update { version,   │                     │                │
│    │  binary_url, sha256 }│                     │                │
│    │                      │                     │                │
│    │                      │──UpdateAgent {      │                │
│    │                      │  binary_url,        │                │
│    │                      │  checksum_sha256,   │                │
│    │                      │  version }──────────►│                │
│    │                      │                     │                │
│    │                      │                     │──download      │
│    │                      │                     │  binary        │
│    │                      │                     │──verify        │
│    │                      │                     │  SHA-256       │
│    │                      │                     │──atomic        │
│    │                      │                     │  replace       │
│    │                      │                     │──self-restart  │
│    │                      │                     │                │
│    │                      │◄──Register {        │                │
│    │                      │   version: "new" }──│                │
│    │                      │                     │                │
│    │                      │  if health fails:   │                │
│    │                      │                     │──rollback to   │
│    │                      │                     │  previous      │
│    │                      │                     │  binary        │
│    │                      │                     │                │
│  Dashboard shows:                                                │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ Agent         │ Current │ Target  │ Status              │     │
│  ├───────────────┼─────────┼─────────┼─────────────────────┤     │
│  │ prd-oracle-01 │ 0.2.0   │ 0.3.0   │ ✓ Updated          │     │
│  │ prd-web-01    │ 0.2.0   │ 0.3.0   │ ↻ Downloading      │     │
│  │ prd-batch-01  │ 0.1.0   │ 0.3.0   │ ⚠ Rollback (fail) │     │
│  │ dr-oracle-01  │ 0.2.0   │ 0.3.0   │ ○ Pending          │     │
│  └───────────────┴─────────┴─────────┴─────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘
```

---

## 13. Certificate Lifecycle Management

**Inspired by:** HashiCorp Consul + Vault PKI (auto-rotation with short TTL)

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│            CERTIFICATE LIFECYCLE MANAGEMENT                      │
│                                                                  │
│  ┌──────────┐   CSR    ┌──────────┐   Sign    ┌──────────────┐ │
│  │  Agent   │─────────►│ Backend  │──────────►│ CA (Vault    │ │
│  │          │◄─────────│ /Gateway │◄──────────│  or built-in)│ │
│  │          │  New Cert │          │  Signed   │              │ │
│  └──────────┘          └──────────┘  Cert     └──────────────┘ │
│                                                                  │
│  Auto-Rotation Flow:                                             │
│  1. Agent checks cert expiry on startup and every 6 hours        │
│  2. If cert expires within renewal_before (default: 7 days):     │
│     a) Generate new private key + CSR                            │
│     b) Send CSR to backend via CertificateRenewal message        │
│     c) Backend forwards to CA, receives signed cert              │
│     d) Backend sends new cert to agent                           │
│     e) Agent atomically replaces cert files                      │
│     f) Agent reconnects with new cert                            │
│  3. Old cert is kept as fallback until new cert is verified       │
│                                                                  │
│  Monitoring:                                                     │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Alert Level    │ Condition                                 │  │
│  ├────────────────┼───────────────────────────────────────────┤  │
│  │ INFO           │ Cert renewed successfully                 │  │
│  │ WARNING        │ Cert expires in < 7 days                  │  │
│  │ CRITICAL       │ Cert expires in < 24 hours                │  │
│  │ ALERT          │ Cert expired — agent connection rejected  │  │
│  └────────────────┴───────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 14. Configuration Security

**Threat addressed:** T9 (Default Credentials in Production)

### Changes

- `JWT_SECRET` has **no default value** — backend refuses to start without it
- `DATABASE_URL` has **no default value** — backend refuses to start without it
- Startup validation checks for known-insecure values and logs CRITICAL warnings
- SAML/OIDC secrets follow the same pattern — required when enabled

```
┌──────────────────────────────────────────────────────────────────┐
│               STARTUP SECURITY VALIDATION                        │
│                                                                  │
│  Backend Startup                                                 │
│     │                                                            │
│     ├─ JWT_SECRET set?                                           │
│     │  ├─ NO → FATAL: "JWT_SECRET must be set" → exit(1)        │
│     │  └─ YES → contains "dev" or "change" or "secret"?         │
│     │           └─ YES → WARN: "Insecure JWT_SECRET detected"   │
│     │                                                            │
│     ├─ DATABASE_URL set?                                         │
│     │  ├─ NO → FATAL: "DATABASE_URL must be set" → exit(1)      │
│     │  └─ YES → contains "localhost" and password "appcontrol"?  │
│     │           └─ YES → WARN: "Default database credentials"   │
│     │                                                            │
│     ├─ OIDC enabled?                                             │
│     │  └─ YES → OIDC_CLIENT_SECRET set? Required.               │
│     │                                                            │
│     └─ SAML enabled?                                             │
│        └─ YES → SAML_CERT_FILE set? Required.                   │
│                                                                  │
│  Environment: APP_ENV=production triggers strict mode            │
│  In strict mode, warnings become fatal errors.                   │
└──────────────────────────────────────────────────────────────────┘
```

---

## 15. Competitive Benchmarking

### Scorecard After Implementation

| Domain | Before | After | Target (Best in Class) | Reference |
|--------|--------|-------|----------------------|-----------|
| Agent Identity | 3/10 | 8/10 | BigFix (10) | Cert binding + fingerprint |
| Message Reliability | 5/10 | 8/10 | BigFix (9) | Seq + Ack + retransmit |
| Gateway Failover | 4/10 | 8/10 | Consul (9) | Multi-URL + ordered failover |
| WebSocket Security | 5/10 | 9/10 | - | Permission-checked subscribe |
| Process Execution | 6/10 | 9/10 | Ansible (9) | Detached + resource limits |
| Rate Limiting | 0/10 | 8/10 | Standard | Per-IP + per-user + per-op |
| Approval Workflow | 0/10 | 8/10 | ServiceNow (9) | Risk-based 4-eyes |
| Break-Glass | 0/10 | 7/10 | ServiceNow (8) | Pre-provisioned + timed |
| Credential Vault | 2/10 | 7/10 | Ansible (9) | Use-but-don't-See + Vault |
| Agent Update | 0/10 | 7/10 | Automic (9) | Push + verify + rollback |
| Cert Lifecycle | 4/10 | 7/10 | Consul (10) | CSR + auto-rotation |
| Config Security | 3/10 | 9/10 | Standard | Require env vars, no defaults |
| **Overall** | **3.1** | **7.9** | | |

### AppControl Unique Differentiators (No Competitor Has These)

1. **DAG-aware sequencing** — start/stop applications respecting dependency order
2. **3-level diagnostic model** — Health / Integrity / Infrastructure
3. **6-phase DR switchover** — managed application-level failover
4. **Process detachment** — double-fork + setsid, processes survive agent crash
5. **Late-binding host→agent resolution** — components reference hostnames, agents bind at registration

---

*This document is versioned in git alongside the code it describes.*
