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

## Maturité déclarée par la source (jamais imposée)

*Méthodologie phase 1 + phase 3 §4.5.*

| Capacité | Statut | Référence code |
|---|---|---|
| Import JSON v4 : `default_knowledge_status` + `default_confidence_score` au niveau request | :material-check-circle: livré | `crates/backend/src/api/import.rs::JsonImportRequest` |
| Import JSON v4 : `knowledge_status` + `confidence_score` par composant / dépendance | :material-check-circle: livré | `V4Component` et `V4Dependency` |
| Helper partagé `integrations::apply_default_maturity` (Components / Dependencies / Both) | :material-check-circle: livré | `crates/backend/src/integrations/mod.rs` |
| CMDB / XL / flow payloads acceptent `default_knowledge_status` + `default_confidence_score` | :material-check-circle: livré | leurs structs `*Payload` |
| Endpoints CSV exposent `?knowledge_status=…&confidence_score=…` | :material-check-circle: livré | `api/ingestion.rs::CsvIngestQuery` |
| Git push transporte la maturité par composant et par dépendance | :material-check-circle: livré | `api/git.rs::fetch_component_knowledge` + `fetch_dependency_knowledge` |
| Wizard frontend expose le sélecteur de maturité | :material-check-circle: livré | `frontend/src/components/captation/IngestionWizard.tsx` |

## Captation multi-sources

*Méthodologie phase 1.*

| Connecteur | Statut | Endpoint |
|---|---|---|
| Agents discovery (active observation) | :material-check-circle: livré | `crates/agent/src/discovery/` |
| CMDB ingestion (JSON) | :material-check-circle: livré | `POST /api/v1/ingestion/cmdb` |
| CMDB ingestion (CSV) | :material-check-circle: livré | `POST /api/v1/ingestion/cmdb/csv` |
| XL Release / XL Deploy ingestion (JSON) | :material-check-circle: livré | `POST /api/v1/ingestion/xl` |
| XL Release / XL Deploy ingestion (CSV) | :material-check-circle: livré | `POST /api/v1/ingestion/xl/csv` |
| Référentiel de flux ingestion (JSON) | :material-check-circle: livré | `POST /api/v1/ingestion/flows` |
| Référentiel de flux ingestion (CSV) | :material-check-circle: livré | `POST /api/v1/ingestion/flows/csv` |
| ITSM / incidents ingestion (JSON) | :material-check-circle: livré | `POST /api/v1/ingestion/incidents` |
| ITSM / incidents ingestion (CSV) | :material-check-circle: livré | `POST /api/v1/ingestion/incidents/csv` |
| **ServiceNow pull** (Table API natif) | :material-check-circle: livré | `POST /api/v1/ingestion/pull/servicenow` |
| **Jira Service Management pull** (JQL natif) | :material-check-circle: livré | `POST /api/v1/ingestion/pull/jira` |

## IA — validation, suggestion, analyse

*Méthodologie phases 1.5, 3, 5.*

| Capacité | Statut | Endpoint |
|---|---|---|
| Provider abstraction (Stub / Anthropic / OpenAI / on-prem) | :material-check-circle: livré | `crates/backend/src/ai/provider.rs` |
| **Provider Anthropic** (Messages API v2023-06-01) | :material-check-circle: livré | env `ANTHROPIC_API_KEY` + `ANTHROPIC_MODEL` |
| **Provider OpenAI / Azure OpenAI** (Chat Completions) | :material-check-circle: livré | env `OPENAI_API_KEY` + `OPENAI_MODEL` + `OPENAI_BASE_URL` |
| Validation IA des schémas (vision LLM) | :material-check-circle: livré | `POST /api/v1/ai/schema/validate` |
| Génération initiale de map par IA | :material-check-circle: livré | `POST /api/v1/ai/map/suggest` |
| Analyse causale d'incidents par IA | :material-check-circle: livré | `POST /api/v1/ai/incident/analyze` |
| **RAG sur runbooks** (BM25-lite local) | :material-check-circle: livré | `POST /api/v1/ai/rag/query` · env `RAG_CORPUS_DIR` |

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
| Webhook `operation` event dispatché en fin de start/stop | :material-check-circle: livré | dans `apps::start_app` / `apps::stop_app` |

## Pattern catalog &amp; capitalisation transversale

*Méthodologie phase 5.*

| Capacité | Statut | Référence code |
|---|---|---|
| Component catalog (structure + endpoints) | :material-check-circle: livré | `crates/backend/src/api/catalog.rs` |
| Profiles (ensembles de checks réutilisables) | :material-check-circle: livré | `crates/backend/src/api/profiles.rs` |
| Incidents table (capitalisation post-incident) | :material-check-circle: livré | `migrations/V059__incidents.sql` |
| Pattern library (templates par techno, usage_count, lien incident) | :material-check-circle: livré | `migrations/V060__pattern_templates.sql` · `api/patterns.rs` |
| **Propagation auto patterns** vers apps similaires | :material-check-circle: livré | `GET /patterns/:id/candidates` · `POST /patterns/:id/propagate` |
| Population auto du catalogue par IA (RAG + suggestions) | :material-check-circle: livré | provider Anthropic / OpenAI requis |

## GitOps &amp; versionnement de map

| Capacité | Statut | Référence code |
|---|---|---|
| Versionnement interne (config_versions) | :material-check-circle: livré | tables append-only, snapshots avant/après |
| Diff entre versions de map | :material-check-circle: livré | `GET /api/v1/apps/:id/dependency-history` |
| Mode PR-only au niveau application | :material-check-circle: livré | via `activation_level = 3` |
| **Intégration Git native** (synchro map vers repo externe) | :material-check-circle: livré | `migrations/V061__git_remotes.sql` · `api/git.rs` · `integrations/git.rs` |
| Providers : GitHub, **GitLab**, **Gitea** | :material-check-circle: livré | three impls in `integrations/git.rs` |

## Annotations &amp; avancement de la connaissance

*Méthodologie phase 3 (revue) + phase 4 (exploitation).*

| Capacité | Statut | Référence code |
|---|---|---|
| **Annotations** (notes, reviews, todos, warnings) sur composants/dépendances/applications | :material-check-circle: livré | `migrations/V062` · `api/annotations.rs` |
| **Confidence score** par composant et dépendance (0..1) | :material-check-circle: livré | colonne ajoutée par V062 |
| **Knowledge status** (candidate → draft → reviewed → validated → deprecated) | :material-check-circle: livré | colonne ajoutée par V062 |
| Endpoint de mise à jour `PUT /components/:id/knowledge` et `PUT /dependencies/:id/knowledge` | :material-check-circle: livré | `api/knowledge.rs` |
| `GET /apps/:id/knowledge/summary` — couverture validée par status | :material-check-circle: livré | `api/knowledge.rs::app_knowledge_summary` |
| **Frontend : badges + panel + summary card** | :material-check-circle: livré | `KnowledgeBadge`, `KnowledgeSummaryCard`, `AnnotationsPanel` |

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
