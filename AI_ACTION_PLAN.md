# Plan d'action — AppControl AI-first : du plan de contrôle déterministe à l'ops agentique souveraine

> **Objet** : mettre le produit à niveau *dès maintenant* pour en faire la solution de référence de gestion de production régulée, agentique et souveraine. Ce document est le plan d'implémentation découlant de l'analyse stratégique et **intègre toutes les décisions arbitrées** (voir tableau §2).

## 1. Thèse produit (le récit qui tient)

> **L'IA raisonne, propose et explique. Le plan de contrôle déterministe (FSM, RBAC, `action_log`, approbations, tables append-only) exécute et trace.**

On ne remplace **rien** de ce qui fait la conformité d'AppControl. On ajoute un **cerveau** au-dessus des *yeux* (découverte) et des *mains* (executor/FSM) déjà en place. Trois piliers différenciants, non occupés sur le segment régulé :

1. **Routeur d'inférence souverain hybride** — on ne parie pas sur un modèle, on est *l'abstraction* qui choisit le bon modèle par tâche selon la sensibilité de la donnée. Anti-obsolescence + DORA-compatible.
2. **Cadran d'autonomie** — l'autonomie est une *variable* (confiance × blast-radius × permission), pas un niveau figé. Défaut : semi-auto avec approbation. Le client monte le cadran à mesure que la confiance croît.
3. **Map niveau architecte, multi-agent** — agrégation des fragments de tous les agents + passe IA d'abstraction qui distingue une vraie application d'un process système. Test de crédibilité en démo.

## 2. Décisions arbitrées → traçabilité dans le plan

| Remarque / décision | Où c'est traité |
|---|---|
| Inférence **hybride pluggable** : LLM en ligne **et** on-prem | Phase 0 — `crates/ai`, trait `InferenceProvider` |
| **Souveraineté primordiale** : rien de sensible ne sort | Phase 0 — `SensitivityClassifier` + `Redactor` (anonymisation *sur la machine* avant envoi frontier) |
| Bénéficier des **dernières technos en ligne** | Phase 0 — provider frontier (Anthropic/Azure OpenAI/Bedrock) routé sur données non sensibles |
| Administrer un **SI complexe entier** | Phases 1→5 — orchestrateur agentique sur toute la chaîne backend→agents |
| **Autonomie = cadran**, pas un niveau | Phase 5 — `ai/autonomy.rs`, gate paramétrable + kill-switch |
| Map construite par **plusieurs agents** → agrégation | Phase 3 — moteur de réconciliation cross-agent |
| Schémas **niveau architecte**, lisibles, pas trop détaillés | Phase 3 — niveaux L0/L1/L2, masquage du bruit système |
| **Distinguer vraie application vs process système** | Phase 3 — classifieur IA (priors = `tech_patterns`) + scoring de confiance |
| Détecter/ajuster les **mauvaises lignes de commande** | Phase 4 — drift detection agent + correction proposée → approbation |
| Diagnostic / comportement machine / logs | Phase 2 — RCA IA + collecte de preuves allow-listée |
| **DORA / sécurité** de bout en bout | Cross-cutting §9 — `ai_decisions` append-only, RBAC, allow-lists, dry-run, blast-radius |

## 3. Principes non négociables (hérités de CLAUDE.md, on ne déroge pas)

- **L'IA n'a aucun privilège propre.** Elle passe par les *mêmes* handlers RBAC que les humains. Pas de canal d'exécution parallèle.
- **Log-before-execute** maintenu : toute action issue de l'IA → `action_log` AVANT exécution, + `ai_decisions` (nouveau, append-only).
- **Dual PostgreSQL 16 / SQLite** : chaque requête garde ses variantes `#[cfg(feature)]`. Le stockage vectoriel a une stratégie par backend (§9.2).
- **Aucun secret/seed en dur** : tous les réglages IA via env (`AI_*`), documentés dans `.env.example` et `docker-compose`.
- **mTLS partout**, tables événementielles **append-only**, snapshots `config_versions` sur tout changement.
- **E2E déploie tous les composants** (backend + gateway + agent), Postgres ET SQLite.

## 4. Architecture cible (vue d'ensemble)

```
                         ┌──────────────────────────────────────────┐
                         │  crates/ai  (NOUVEAU)                      │
 Frontend Copilot  ───▶  │  Routeur d'inférence souverain            │
 CLI / MCP         ───▶  │   ├─ SensitivityClassifier                │
                         │   ├─ Redactor (anonymise avant frontier)  │
                         │   ├─ InferenceProvider (trait)            │
                         │   │    • LocalProvider (vLLM/Ollama)       │
                         │   │    • FrontierProvider (Anthropic/Azure)│
                         │   └─ EmbeddingProvider + VectorMemory      │
                         └───────────────┬──────────────────────────┘
                                         │ (appelle les handlers existants via RBAC)
        backend/src/ai (NOUVEAU)         ▼
        ├─ orchestrator.rs  ── boucle Observe→Reason→Plan→Gate→Act→Verify
        ├─ diagnostics.rs   ── RCA (Phase 2)
        ├─ architect.rs     ── agrégation + abstraction map (Phase 3)
        ├─ autonomy.rs      ── cadran (Phase 5)
        └─ memory.rs        ── RAG incidents
                                         │
            ┌────────────────────────────┼───────────────────────────┐
            ▼ (protocole mTLS existant)   ▼                            ▼
        Gateway  ──────────────▶  Agent (capteur + acteur sûr)   action_log / ai_decisions
                                  ├─ discovery/ (fragments)        (append-only)
                                  ├─ evidence (collecte allow-list)
                                  └─ drift detection (Phase 4)
```

L'orchestration IA vit **dans le backend** (le gros LLM ne descend pas dans l'agent). L'agent reste léger : capteur + acteur sûr + triage local optionnel.

---

## 5. Phase 0 — Socle IA : le routeur d'inférence souverain

**But** : poser la fondation conforme. Aucune fonctionnalité visible, mais tout en dépend.

### Nouveau crate `crates/ai/`
- `Cargo.toml` : ajout au workspace (`members`). Deps : `reqwest` (rustls), `serde`, `tokio`, `async-trait`, `thiserror`.
- `src/provider.rs` — trait `InferenceProvider { async fn complete(req) ; async fn embed(texts) }`.
  - `LocalProvider` — API OpenAI-compatible (vLLM/Ollama/TGI), URL via `AI_LOCAL_BASE_URL`.
  - `FrontierProvider` — Anthropic Messages API / Azure OpenAI / Bedrock, sélection via `AI_FRONTIER_PROVIDER`.
- `src/sensitivity.rs` — `SensitivityClassifier` : note un contexte `Public | Internal | Sensitive | Secret` (regex secrets + heuristiques chemins/`is_sensitive` déjà présent côté MCP).
- `src/redactor.rs` — `Redactor::redact()` : masque secrets/IP/hostnames internes et produit une vue *abstraite* envoyable à un provider frontier.
- `src/router.rs` — `InferenceRouter::route(task, sensitivity) -> &dyn InferenceProvider`. Règle par défaut : `Sensitive|Secret → Local` ; `Public|Internal → Frontier` (configurable, override par tenant).
- `src/memory.rs` — `VectorMemory` (embeddings + recherche cosine) ; abstraction de stockage (§9.2).

### Backend
- `crates/backend/src/ai/mod.rs` (NOUVEAU) — câble le routeur, exposé à `api`.
- `crates/backend/src/config.rs` — ajouter `AiConfig` lisant les env (auto-documenté par `gen_configuration.py`).

### Migration `V057__ai_decisions.sql` (append-only)
```sql
CREATE TABLE ai_decisions (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL,
    actor_user_id UUID,                 -- qui a déclenché / NULL si autonome
    kind VARCHAR(40) NOT NULL,          -- 'chat','rca','architect','command_fix','remediation'
    model_provider VARCHAR(40) NOT NULL,
    model_name VARCHAR(120) NOT NULL,
    sensitivity VARCHAR(20) NOT NULL,
    routed_to VARCHAR(20) NOT NULL,     -- 'local' | 'frontier'
    prompt_hash CHAR(64) NOT NULL,      -- reproductibilité sans stocker le secret
    context_summary JSONB NOT NULL,
    proposed_plan JSONB,
    confidence REAL,
    blast_radius VARCHAR(20),
    approved_by UUID,
    outcome VARCHAR(20),                -- 'proposed','approved','executed','rejected','failed'
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- APPEND-ONLY : aucun UPDATE/DELETE (cf. règle CLAUDE.md #2)
```
*(variante SQLite : `CHAR(36)` pour UUID via `DbUuid`, `TEXT` pour JSONB via `DbJson`.)*

### Env (→ `.env.example` + `docker-compose*.yml`)
```
AI_INFERENCE_MODE=hybrid            # local | frontier | hybrid
AI_LOCAL_BASE_URL=http://vllm:8000/v1
AI_LOCAL_MODEL=...
AI_FRONTIER_PROVIDER=anthropic      # anthropic | azure_openai | bedrock | none
AI_FRONTIER_API_KEY=
AI_FRONTIER_MODEL=
AI_EMBEDDING_MODEL=
AI_REDACTION_ENABLED=true
AI_KILL_SWITCH=false
AI_AUTONOMY_DEFAULT=approval        # advisory | approval | policy
```

**Tests** : routage par sensibilité, redaction (aucun secret ne passe), provider mock. **Démo** : aucune (socle). **Vérif** : build + clippy + migration up/down Postgres & SQLite.

---

## 6. Phase 1 — Copilote conversationnel (read-only, risque nul)

**But** : la démo « waouh » sans aucune action sur la prod.

- `crates/backend/src/ai/orchestrator.rs` — boucle de tool-calling **réutilisant les handlers existants** (topology, logs, incidents, status). Réutilise la logique déjà écrite dans `crates/mcp/src/tools.rs` (list_apps, get_app_status, get_topology, get_component_logs, search_logs, get_incidents…) en l'exposant comme *outils internes*, chaque appel passant par RBAC.
- `crates/backend/src/api/ai.rs` (NOUVEAU) — `POST /api/ai/chat` en **SSE streaming**. Vérifie la permission `view` avant tout, journalise dans `ai_decisions` (`kind='chat'`).
- `crates/backend/src/api/mod.rs` — monter la route.
- Frontend :
  - `frontend/src/pages/AiCopilotPage.tsx` (NOUVEAU) + entrée de navigation.
  - `frontend/src/api/ai.ts` (NOUVEAU) — client SSE.
  - Panneau copilote ancrable réutilisable (composant `AiCopilot/`).
- Outils **strictement read-only** dans cette phase (aucun start/stop exposé au LLM).

**Tests** : orchestrateur avec provider mock, refus si permission manquante, SSE. **E2E** : question → réponse fondée sur la topo réelle (backend+gateway+agent up). **Démo** : « Pourquoi la chaîne paiement est-elle dégradée ? » → réponse argumentée.

---

## 7. Phase 2 — Diagnostic IA / RCA (réduction du MTTR)

**But** : sur incident, investigation automatique et rapport de cause racine.

- **Collecte de preuves allow-listée** : réutiliser les messages protocole existants `GetProcessLogs`, `GetFileLogs`, `GetEventLogs`, `ExecuteDiagnosticCommand` (déjà dans `ServerMessage`). Optionnel : ajouter un variant batch `EvidenceRequest`/`EvidenceReport` dans `crates/common/src/protocol.rs` si le nombre d'allers-retours le justifie. **Jamais** de shell arbitraire dicté par le LLM — uniquement les commandes de diagnostic déjà déclarées (`integrity_check_cmd`, `infra_check_cmd`, `native_command_specs`).
- `crates/backend/src/ai/diagnostics.rs` (NOUVEAU) — déclenché par `core/fsm.rs` sur transition `FAILED|DEGRADED`. Corrèle `state_transitions`, `check_events`, les logs récents, et la **mémoire vectorielle** des incidents similaires (RAG). Produit un rapport : hypothèses classées + confiance + remédiation proposée.
- Hook léger dans `crates/backend/src/core/fsm.rs` / `core/diagnostic.rs` → enqueue d'un job d'investigation (non bloquant pour la FSM).
- Migration `V058__ai_rca_reports.sql` (append-only) : `incident_ref`, `component_id`, `hypotheses JSONB`, `recommended_action JSONB`, `confidence`, `decision_id` (FK `ai_decisions`).
- Frontend : encart RCA sur `SupervisionPage.tsx` / détail composant (hypothèses + bouton « préparer la remédiation »).

**Tests** : génération RCA déterministe avec provider mock + fixtures d'événements ; le RAG remonte le bon incident passé. **Démo** : on tue un process → RCA auto en quelques secondes. **DORA** : tout dans `ai_rca_reports` + `ai_decisions`.

---

## 8. Phase 3 — Map niveau architecte, multi-agent (crédibilité)

**But** : transformer N machines isolées en **un schéma d'architecture lisible**, pas un dump de process.

### Agrégation cross-agent
- Promouvoir la corrélation de `crates/backend/src/api/discovery/` en **moteur de réconciliation** : recoud les fragments de découverte de tous les agents en un seul graphe (connecteur TCP sortant d'un agent ↔ listener d'un autre ; endpoints de config ; typage par port). Gestion des conflits/duplicats.

### Passe « architecte » par l'IA (le différenciateur)
- `crates/backend/src/ai/architect.rs` (NOUVEAU), trois jobs que la regex seule rate :
  1. **App vs process système** : classer (`systemd`, `cron`, `sshd`, agents monitoring = bruit) vs application métier. Priors = `crates/agent/src/discovery/tech_patterns.rs` ; scoring de confiance.
  2. **Regroupement + nommage métier** : *6 process Java + 1 PG + 1 Redis sur 3 serveurs* → « Plateforme de paiement ».
  3. **Niveaux de détail** : **L0** applications / **L1** composants / **L2** process & ports.
- **Multi-agent de raisonnement** : un sous-appel LLM résume *par hôte* (donnée locale, routée `local` si sensible) → un appel **agrégateur** réconcilie et arbitre. Cohérent avec la vision multi-agent.

### Types & stockage
- `crates/common/src/types.rs` — `ArchitectureView { tiers, nodes, edges, confidence_per_edge }`, `ArchNodeKind { Application, Component, SystemProcess }`.
- Migration `V059__architecture_view.sql` : vue abstraite + confiance, lié au draft de découverte existant.

### Frontend
- `frontend/src/pages/MapViewPage.tsx` / `DiscoveryPage.tsx` :
  - **Zoom progressif L0→L1→L2** (par défaut L0–L1 : lisible en 10 s).
  - Toggle « masquer le bruit système ».
  - Badges de confiance par nœud/arête.
  - Validation humaine du draft (le wizard existe déjà).

**Tests** : classification app/système sur fixtures, regroupement, réconciliation cross-agent, niveaux. **E2E** : 3 agents → un graphe L0 cohérent. **Démo** : « voici votre SI en une carte d'architecte », pas un htop.

---

## 9. Phase 4 — Correction de commandes (drift detection + ajustement)

**But** : le point clé — l'agent est sur la machine, il sait si la commande start/stop/check est mauvaise et propose la bonne.

- **Agent** (`crates/agent/src/executor.rs` + nouveau `drift.rs`) : quand une commande déclarée échoue (ou que le `check_cmd` ne matche plus), recollecte le contexte (service renommé ? `cwd` changé ? binaire déplacé ? unit systemd absente ?) et émet un `CommandDriftReport`.
- **Protocole** (`crates/common/src/protocol.rs`) : `AgentMessage::CommandDriftReport { component_id, failed_cmd, observed_context }`.
- **Backend** (`crates/backend/src/ai/orchestrator.rs`) : l'IA propose la commande corrigée **avec justification + diff**. La proposition entre dans le **flux d'approbation existant** (`api/approvals.rs`). À l'approbation → mise à jour de `component_commands` via **`config_versions`** (snapshot avant/après — règle #10).
- **Frontend** : revue du drift dans la page approbations (diff commande, justification, confiance).

**Tests** : détection drift (service renommé), proposition correcte, snapshot config créé, refus sans approbation. **E2E** : on renomme un service → drift remonté → fix proposé → approuvé → start réussit. **DORA** : `ai_decisions` (`kind='command_fix'`) + `config_versions` + `action_log`.

---

## 10. Phase 5 — Cadran d'autonomie + remédiation

**But** : rendre l'autonomie *configurable*, du conseil à la remédiation policy-driven, sans jamais perdre la trace.

- `crates/backend/src/ai/autonomy.rs` (NOUVEAU) — calcule la **porte** : `gate(action) = f(confidence, blast_radius, rbac_permission)`.
  - `advisory` : l'IA propose, l'humain fait tout.
  - `approval` (**défaut**) : plan complet → approbation 1-clic → la FSM exécute et trace.
  - `policy` : pour scénarios pré-validés et bornés, action sans humain (kill-switch + seuils).
- **Blast-radius** : réutiliser `core/dag.rs` pour calculer l'impact (nb composants affectés) avant exécution.
- **Exécution** : *toujours* via la FSM + `sequencer.rs` existants. L'IA ne fait que produire le plan ; le moteur déterministe agit. `action_log` + `ai_decisions` (`kind='remediation'`).
- **Kill-switch global** : `AI_KILL_SWITCH` + toggle UI (coupe toute autonomie instantanément).
- Migration `V060__autonomy_policies.sql` : seuils d'autonomie par app/composant.
- **Frontend** : réglages d'autonomie (le « cadran ») par application + indicateur d'état global.

**Tests** : gate refuse une action haut blast-radius en mode approval ; kill-switch coupe tout ; policy n'agit que sous seuil. **E2E** : incident → plan → approbation → remédiation → vérif retour `RUNNING`. **Démo** : on monte le cadran, l'IA remédie un incident borné de bout en bout, tout est tracé.

---

## 11. Cross-cutting (à respecter à chaque phase)

### 11.1 Sécurité & DORA
- `ai_decisions` / `ai_rca_reports` / `autonomy_policies` : **append-only**, reproductibilité totale (qui, quel modèle, quelle version, quel plan, qui a approuvé, résultat).
- L'IA n'a **aucun privilège** : tout passe par RBAC + allow-lists de commandes.
- Dry-run obligatoire pour actions destructrices ; blast-radius calculé avant exécution ; secrets masqués (`Redactor`) avant tout provider frontier.

### 11.2 Dual PostgreSQL / SQLite (stockage vectoriel)
- **Postgres 16** : extension `pgvector` (colonne `vector`), index `ivfflat`/`hnsw`.
- **SQLite** : embeddings stockés en `BLOB`/`DbJson`, **cosine en Rust** (volumes d'une mémoire d'incidents = OK). Abstraction dans `crates/ai/src/memory.rs` pour masquer la différence.
- Chaque requête PG-spécifique garde sa variante `#[cfg(feature = "sqlite", not(feature = "postgres"))]`.

### 11.3 Documentation auto-générée
- Nouveaux env `AI_*` dans `config.rs` → `gen_configuration.py` les capte automatiquement.
- Nouvelles migrations V057→V060 → `gen_database_schema.py`.
- Nouveaux enums (`ArchNodeKind`, modes d'autonomie) dans `types.rs` → `gen_enums.py`.
- Nouveaux outils IA exposés au MCP → étendre `gen_mcp.py` si la signature change.
- **On ne touche aucun markdown de référence à la main.**

### 11.4 CI & validation (après chaque phase)
1. `cargo build --workspace`
2. `cargo test --workspace`
3. `cargo clippy --workspace -- -D warnings`
4. `cd frontend && npm run build && npm test`
5. Migrations up/down sur Postgres **et** SQLite
6. E2E (backend + gateway + agent réels) pour les phases qui touchent l'exécution
7. CI verte avant de passer à la phase suivante.

---

## 12. Séquencement & jalons

| Phase | Livrable démo-able | Dépend de |
|---|---|---|
| **0** Socle IA | (interne) routeur + `ai_decisions` | — |
| **1** Copilote read-only | Q&A fondée sur la prod réelle | 0 |
| **2** Diagnostic / RCA | RCA auto sur incident | 0 |
| **3** Map architecte multi-agent | SI en carte d'architecte | 0 |
| **4** Correction de commandes | drift → fix approuvé → start OK | 0,1 |
| **5** Cadran d'autonomie | remédiation bout-en-bout tracée | 0–4 |

Chaque phase est livrable et démontrable seule, ne casse aucune règle existante, et alimente directement le récit de levée (copilote = waouh ; RCA = MTTR ; map = crédibilité ; cadran = disruption maîtrisée).

---

## 13. Première itération concrète à lancer

1. Créer `crates/ai/` (trait `InferenceProvider`, `LocalProvider`, `FrontierProvider`, `router`, `sensitivity`, `redactor`) + l'ajouter au workspace.
2. Migration `V057__ai_decisions.sql` (Postgres + variante SQLite).
3. `AiConfig` dans `backend/src/config.rs` + env dans `.env.example`/`docker-compose`.
4. `backend/src/ai/orchestrator.rs` minimal réutilisant les outils read-only de `crates/mcp/src/tools.rs`.
5. `POST /api/ai/chat` (SSE) + `AiCopilotPage.tsx`.
6. Tests + CI verte → **démo copilote**.
