---
title: Implementation status
description: What is shipped today versus what the strategic docs promise.
---

# Implementation status

Factual inventory of what is **concretely shipped** in the AppControl
codebase, organised by methodology phase. Cross-checked against the
four strategic documents
([strategy](strategy.html),
[methodology](methodology.html),
[vision](vision.html),
[pricing](pricing.html)).

Legend:

- :material-check-circle:{ .ok } **livré** — code complete and tested
- :material-flask:{ .stub } **stub** — stable API contract, placeholder implementation
- :material-progress-clock:{ .todo } **à venir** — not started

## Adoption graduelle — 5 niveaux d'activation

*Méthodologie phase 4.*

| Capacité | Statut | Référence code |
|---|---|---|
| Flag `activation_level` par application (0..4) | :material-check-circle: livré | `migrations/V058__application_activation_level.sql` |
| Helpers d'enforcement (`check_runtime_ops_allowed`, `require_diagnostic`) | :material-check-circle: livré | `crates/backend/src/core/activation.rs` |
| API `GET / PUT /api/v1/apps/:id/activation` | :material-check-circle: livré | `crates/backend/src/api/activation.rs` |
| Mode PR-only (header `X-PR-Approved-Sha`) | :material-check-circle: livré | enforcement dans start_app, stop_app, branch, start_to, rebuild, switchover |
| Page frontend de pilotage + badge | :material-check-circle: livré | `frontend/src/pages/ActivationPage.tsx` · `components/ActivationBadge.tsx` |

## Captation multi-sources

*Méthodologie phase 1.*

| Connecteur | Statut | Endpoint |
|---|---|---|
| Agents discovery (active observation) | :material-check-circle: livré | `crates/agent/src/discovery/` |
| CMDB ingestion (générique JSON) | :material-check-circle: livré | `POST /api/v1/ingestion/cmdb` |
| XL Release / XL Deploy ingestion | :material-check-circle: livré | `POST /api/v1/ingestion/xl` |
| Référentiel de flux ingestion | :material-check-circle: livré | `POST /api/v1/ingestion/flows` |
| ITSM / incidents ingestion | :material-check-circle: livré | `POST /api/v1/ingestion/incidents` |
| Connecteurs CSV (en plus du JSON) | :material-progress-clock: à venir | — |
| Connecteurs natifs ServiceNow / Jira SM (pull) | :material-progress-clock: à venir | — |

## IA — validation, suggestion, analyse

*Méthodologie phases 1.5, 3, 5.*

| Capacité | Statut | Endpoint |
|---|---|---|
| Provider abstraction (Stub / Anthropic / OpenAI / on-prem) | :material-check-circle: livré | `crates/backend/src/ai/provider.rs` |
| Validation IA des schémas (vision LLM) | :material-flask: stub | `POST /api/v1/ai/schema/validate` |
| Génération initiale de map par IA | :material-flask: stub | `POST /api/v1/ai/map/suggest` |
| Analyse causale d'incidents par IA | :material-flask: stub | `POST /api/v1/ai/incident/analyze` |
| RAG sur runbooks / historique opérationnel | :material-progress-clock: à venir | — |

!!! note "Stub vs. real provider"
    *Stub* = the API contract is stable and immediately usable by
    frontends and integrators. The default provider returns a
    deterministic response marked `"provider": "stub"`. To switch to
    a real LLM, implement `Provider` in
    `crates/backend/src/ai/provider.rs` and set the `AI_PROVIDER`
    environment variable.

## DR, rebuild, conformité

*Stratégie + méthodologie phase 4.*

| Capacité | Statut | Référence code |
|---|---|---|
| DR switchover 6 phases avec rollback | :material-check-circle: livré | `crates/backend/src/core/switchover/` |
| Rebuild engine (DAG order, bastion infra, protection) | :material-check-circle: livré | `crates/backend/src/core/rebuild.rs` |
| Dry-run sur start / stop / rebuild | :material-check-circle: livré | flag `dry_run: true` sur les endpoints |
| Audit log append-only (action_log, state_transitions, switchover_log) | :material-check-circle: livré | tables protégées par règle critique #2 |
| Reports DORA (RTO, MTTR, compliance) | :material-check-circle: livré | `crates/backend/src/api/reports/` |
| Mesure RTR (Recovery Time for Rebuild) | :material-check-circle: livré | chronométré dans `action_log` par exécution |
| Webhooks (HMAC, circuit breaker, retry) | :material-check-circle: livré | `crates/backend/src/core/notifications.rs` |

## Pattern catalog &amp; capitalisation transversale

*Méthodologie phase 5.*

| Capacité | Statut | Référence code |
|---|---|---|
| Component catalog (structure + endpoints) | :material-check-circle: livré | `crates/backend/src/api/catalog.rs` |
| Profiles (ensembles de checks réutilisables) | :material-check-circle: livré | `crates/backend/src/api/profiles.rs` |
| Incidents table (capitalisation post-incident) | :material-check-circle: livré | `migrations/V059__incidents.sql` |
| Population auto du catalogue par IA | :material-progress-clock: à venir | nécessite un vrai provider AI |
| Propagation auto de PR de remediation à apps similaires | :material-progress-clock: à venir | — |

## GitOps &amp; versionnement de map

| Capacité | Statut | Référence code |
|---|---|---|
| Versionnement interne (config_versions) | :material-check-circle: livré | tables append-only, snapshots avant/après |
| Diff entre versions de map | :material-check-circle: livré | `GET /api/v1/apps/:id/dependency-history` |
| Mode PR-only au niveau application | :material-check-circle: livré | via `activation_level = 3` |
| Intégration Git native (synchro vers repo externe) | :material-progress-clock: à venir | — |

## Pricing &amp; simulateur

| Capacité | Statut | Référence |
|---|---|---|
| Document pricing avec 3 composantes | :material-check-circle: livré | [pricing.html](pricing.html) |
| Simulateur ROI interactif (recalcul temps réel) | :material-check-circle: livré | JavaScript intégré, section 6 |
| Hypothèses par taille &amp; secteur | :material-check-circle: livré | section 8 |
| Modèle de calcul exposé en clair | :material-check-circle: livré | annexe A1 |

## Documents stratégiques

| Document | Question | Statut |
|---|---|---|
| [strategy.html](strategy.html) | Pourquoi AppControl | v0.9 draft |
| [methodology.html](methodology.html) | Comment AppControl | v0.9 draft |
| [vision.html](vision.html) | Jusqu'où AppControl | v0.9 draft |
| [pricing.html](pricing.html) | Combien AppControl | v0.9 draft |
