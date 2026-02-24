# AppControl Positioning Guide

## What AppControl Is

AppControl is an **operational mastery platform** for IT applications. It provides:

- **A single source of truth** for application dependency graphs (DAGs)
- **Real-time health monitoring** via distributed agents
- **Sequenced operations** (start/stop/restart) that respect dependency order
- **DR site switchover** orchestration
- **Full audit trails** compliant with DORA regulations

## What AppControl Is NOT

| AppControl is NOT... | Instead, it... |
|---------------------|----------------|
| A scheduler | Integrates with Control-M, AutoSys, Dollar Universe, TWS |
| A deployment tool | Integrates with XL Release, Jenkins, GitLab CI, ArgoCD |
| A monitoring tool | Complements Datadog, Prometheus, Zabbix with operational context |
| A CMDB | Consumes CMDB data; exposes topology for CMDB enrichment |
| A config management tool | Complements Ansible, Puppet, Terraform |

## The Problem AppControl Solves

### Dependency Duplication

In most enterprises, application dependency knowledge is scattered across:

- **Schedulers** (Control-M jobs, AutoSys chains)
- **Release tools** (XL Release templates, Jenkins pipelines)
- **Runbooks** (Confluence pages, SharePoint docs)
- **Scripts** (Shell scripts, PowerShell, Ansible playbooks)
- **People's heads** (tribal knowledge of senior operators)

Each tool maintains its own partial, often outdated view of dependencies. When a dependency changes, updates are needed in N places. In practice, some are missed, leading to production incidents when processes start in the wrong order.

### The Cost of Duplication

- **Incidents**: Wrong start order causes failures, cascading outages
- **Slow deployments**: Teams fear touching dependency chains, delaying releases
- **Knowledge silos**: Only specific people know the real dependency order
- **Audit gaps**: No single view of what changed, when, and why
- **Operational debt**: Each new tool adds another copy of dependency data

## AppControl's Position in the Ecosystem

```
                    ┌──────────────────────┐
                    │    AppControl         │
                    │  (Source of Truth)    │
                    │                      │
                    │  • DAG Model         │
                    │  • Real-time State   │
                    │  • Operations Engine │
                    │  • Audit Trail       │
                    └──────┬───────────────┘
                           │
            ┌──────────────┼──────────────┐
            │              │              │
    ┌───────▼──────┐ ┌────▼──────┐ ┌─────▼──────┐
    │  Schedulers  │ │  Release  │ │ Monitoring │
    │              │ │  Tools    │ │            │
    │ Control-M    │ │ XL Release│ │ Datadog    │
    │ AutoSys      │ │ Jenkins   │ │ Prometheus │
    │ Dollar Univ. │ │ GitLab CI │ │ Zabbix     │
    │ TWS          │ │ ArgoCD    │ │ Dynatrace  │
    └──────────────┘ └───────────┘ └────────────┘
```

### How Each Tool Interacts with AppControl

**Schedulers** call AppControl to:
- Start/stop applications via CLI (`appctl start myapp --wait`) or REST API
- Query the current state before launching dependent jobs
- Get the correct execution plan without re-defining dependencies

**Release/deployment tools** call AppControl to:
- Restart applications after deployment (respecting dependency order)
- Validate that a proposed restart sequence is correct
- Export the topology to verify deployment prerequisites

**Monitoring tools** receive from AppControl:
- Webhooks on state changes (component UP/DOWN/FAILED)
- DORA compliance metrics
- Availability and incident data

## Coexistence Matrix

| Tool | Relationship | Integration Point |
|------|-------------|-------------------|
| **Control-M** | AppControl starts apps; Control-M schedules when | CLI wrapper: `appctl start <app> --wait` as a Control-M job |
| **AutoSys** | Same as Control-M | CLI wrapper with exit codes (0=OK, 1=FAIL, 2=TIMEOUT) |
| **Dollar Universe** | Same as Control-M | REST API call from DU job |
| **XL Release** | XL Release deploys; AppControl restarts | HTTP task in XL Release template calling AppControl API |
| **Jenkins** | Jenkins builds/deploys; AppControl manages runtime | Post-deploy step: `curl -X POST /api/v1/apps/{id}/start` |
| **Ansible** | Ansible configures; AppControl operates | Ansible module or URI task calling AppControl API |
| **ServiceNow** | ServiceNow tracks changes; AppControl executes | Webhook integration for state change notifications |
| **PagerDuty** | AppControl detects; PagerDuty alerts | Webhook on FAILED state transitions |

## Key Differentiators

### 1. Native DAG Model

AppControl models applications as directed acyclic graphs where components have typed dependencies. This is not an afterthought or a template — it's the core data model.

### 2. Real-Time State Awareness

Before executing any operation, AppControl knows the current state of every component (RUNNING, STOPPED, FAILED, DEGRADED). Smart start skips components already running, handles failed ones specially (pink branch), and starts in parallel where dependencies allow.

### 3. Topology Export API

AppControl's DAG is **consumable** by other tools via REST API:
- `GET /api/v1/apps/{id}/topology?format=json` — Full topology with start/stop order
- `GET /api/v1/apps/{id}/topology?format=yaml` — YAML for config management tools
- `GET /api/v1/apps/{id}/topology?format=dot` — Graphviz DOT for visualization

### 4. Sequence Validation

External tools can **validate** their restart sequences against AppControl's DAG:
- `POST /api/v1/apps/{id}/validate-sequence` — Check if a proposed order is correct
- Returns specific conflicts: "Service A depends on Service B but starts before it"

### 5. Execution Plans Without Execution

Read-only plan computation for validation and approval workflows:
- `GET /api/v1/apps/{id}/plan?operation=start` — What would happen if we start?
- Includes predicted actions per component (start, skip, restart)

### 6. Advisory Mode

Deploy AppControl agents in observation-only mode during migration. Agents monitor health and report state, but don't execute start/stop commands. Lets teams validate the dependency model before going live.

## Migration Scenarios

### From "XL Release for Restarts" to AppControl

1. **Phase 1 — Observe**: Deploy agents in advisory mode alongside XL Release templates
2. **Phase 2 — Validate**: Use validate-sequence API to compare XL Release order with DAG
3. **Phase 3 — Parallel**: XL Release calls AppControl API instead of running scripts directly
4. **Phase 4 — Migrate**: Remove restart logic from XL Release; AppControl handles operations

### From "Shell Scripts" to AppControl

1. **Phase 1 — Model**: Import application topology (YAML import or UI wizard)
2. **Phase 2 — Observe**: Advisory mode agents monitor current state
3. **Phase 3 — Test**: Dry-run mode to validate execution plans
4. **Phase 4 — Operate**: Switch from scripts to AppControl for daily operations

### From "Scheduler-Managed Restarts" to AppControl

1. **Phase 1 — Integrate**: Scheduler jobs call `appctl` CLI instead of custom scripts
2. **Phase 2 — Simplify**: Scheduler delegates dependency logic to AppControl
3. **Phase 3 — Optimize**: Scheduler only schedules *when*; AppControl decides *how*

## Competitive Positioning Summary

> "AppControl occupies the gap between deployment and scheduling — **operational mastery**. Your scheduler knows *when* to run. Your CI/CD knows *what* to deploy. AppControl knows *how* your application works and operates it accordingly. These three work together, each in its domain, with AppControl as the single source of truth for your application's dependency graph."
