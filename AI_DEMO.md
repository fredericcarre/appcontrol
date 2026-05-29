# AI Demo — tester la transformation agentique en une commande

> Objectif : pouvoir **valider les concepts en quelques secondes**, sans monter
> tout le stack. Ce document accompagne la Phase 0 du plan (`AI_ACTION_PLAN.md`).
> Tout ce qui suit a été exécuté et vérifié (`cargo test -p appcontrol-ai` = 11/11).

Ce qui est livré dans cet incrément :

| Brique | Où | Ce que ça prouve |
|---|---|---|
| Routeur d'inférence souverain | `crates/ai/` (`provider`, `sensitivity`, `redactor`, `router`) | Hybride local/frontier ; les secrets ne sortent jamais |
| Passe « architecte » | `crates/ai/src/architect.rs` | Vraie app vs process système ; dépendances ; groupes L0 |
| Découverte standalone | `appcontrol-agent discover` | L'agent scanne sans backend — la brique agentique |
| Démo Docker | `demo/agentic/` | Vraies apps (PG/Redis/nginx/order-api) découvertes puis cartographiées |

---

## 1. Démo zéro-infra (la plus rapide — aucune dépendance, aucune clé API)

```bash
cargo run -p appcontrol-ai -- demo
```

Sortie réelle (échantillon « order platform ») :

```
AppControl — Architecture map (1 application(s), 4 component(s) across 1 host(s))
================================================================

▣ APPLICATION: order-api  (●high)
   ├─ order-api        [Service] java :8080  ●high  @srv-paris-01
   │     └─▶ depends on postgresql (via config — spring.datasource.url)
   │     └─▶ depends on redis-server (via config — spring.redis.host)
   │       start: systemctl start order-api
   ├─ postgresql       [Database] postgresql :5432  ●high  @srv-paris-01
   │       start: systemctl start postgresql
   ├─ redis-server     [Cache] redis :6379  ●high  @srv-paris-01
   ├─ nginx            [Web] nginx :443,:80  ●high  @srv-paris-01
   │     └─▶ depends on order-api (via tcp — 127.0.0.1:8080)

— SYSTEM (filtered): 5 process(es) hidden (systemd, sshd, cron, agent, …)
  This is no longer 'just an agent on a box' — it is an agentic view of the system.
```

C'est la **map niveau architecte** : les applications d'abord, leurs dépendances
(confirmées par config ou TCP), les commandes opérables, et le bruit système
masqué.

## 2. La chaîne agentique RÉELLE sur votre machine (agent → IA)

```bash
cargo run -p appcontrol-agent -- discover --json | cargo run -p appcontrol-ai -- architect
```

L'agent scanne réellement la machine, l'IA en fait une carte. Exemple de sortie
réelle sur un host de dev (52 process système écartés, seul ce qui **écoute
vraiment** est remonté) :

```
AppControl — Architecture map (1 application(s), 1 component(s) across 1 host(s))
...
▣ APPLICATION: tokio-rt-worker  [Service] - :2024,:2025  ●high  @vm
— SYSTEM (filtered): 52 process(es) hidden (systemd, sshd, cron, agent, …)
```

> Détail de crédibilité : la règle est qu'un composant n'est une **application**
> que s'il a une **ancre opérationnelle réelle** (port d'écoute, service système,
> ou commande opérable). C'est ce qui empêche le matcher par nom de promouvoir un
> thread (`Bun Pool 2`) ou un shell (`bash`) en « base de données ».

## 3. Le routeur souverain : où part la donnée ?

```bash
cargo run -p appcontrol-ai -- classify "spring.datasource.password=hunter2"
# sensitivity = Secret
# routing     = pinned to a local/sovereign model — never leaves the machine

cargo run -p appcontrol-ai -- classify "a postgres service on port 5432"
# sensitivity = Internal
# routing     = may go to a frontier model (after redaction)
```

Brancher un vrai modèle est un **choix de config**, pas une réécriture :

```bash
# On-prem souverain (Ollama / vLLM) :
export AI_LOCAL_BASE_URL=http://localhost:11434/v1 AI_LOCAL_MODEL=qwen2.5:14b
# Frontier (seulement pour la donnée non sensible, après redaction) :
export AI_INFERENCE_MODE=hybrid AI_FRONTIER_BASE_URL=... AI_FRONTIER_MODEL=... AI_FRONTIER_API_KEY=...
cargo run -p appcontrol-ai -- demo   # la passe de nommage L0 utilisera le modèle
```

Sans aucune variable, un mock déterministe est utilisé → la démo marche toujours.

## 4. Démo Docker : vraies apps qui tournent, découvertes par l'agent

```bash
docker compose -f demo/agentic/docker-compose.yml up --build
```

Un conteneur lance **PostgreSQL + Redis + nginx + un order-api** réels, puis :
1. l'agent les découvre (`appcontrol-agent discover`),
2. l'IA produit la carte d'architecture (`appcontrol-ai architect`).

Le conteneur reste up ; on peut rejouer la chaîne :

```bash
docker exec -it appcontrol-agentic-demo bash -c \
  'appcontrol-agent discover --json | appcontrol-ai architect'
```

Voir `demo/agentic/README.md` pour les détails.

---

## Vérifier le code

```bash
cargo test  -p appcontrol-ai          # 11 tests (router, redaction, architect…)
cargo clippy -p appcontrol-ai -p appcontrol-agent --all-targets -- -D warnings
```

## 5. Copilote backend (read-only) — `POST /api/v1/ai/chat`

Le backend expose maintenant un copilote conversationnel **read-only**, branché
RBAC (utilisateur authentifié requis) et tracé dans la table append-only
`ai_decisions` (migration `V057`, Postgres + SQLite). L'IA n'a aucun privilège
propre : elle explique et recommande, mais toute action passe par une opération
approuvée.

```bash
curl -s -X POST https://<backend>/api/v1/ai/chat \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"message":"Pourquoi la chaîne paiement est-elle dégradée ?"}'
# → { "answer": "...", "routed_to": "local", "model": "...", "sensitivity": "internal" }
```

`routed_to` rend la souveraineté **transparente** : on voit, par requête, si
l'inférence est restée locale ou est allée vers un modèle frontier (après
redaction). Le kill-switch global `AI_KILL_SWITCH=true` désactive tout (503).

## Et après (prochains incréments du plan)

Cet incrément est volontairement **standalone** (pas de backend, pas de DB) pour
valider les concepts vite. Les suivants (cf. `AI_ACTION_PLAN.md`) branchent ces
briques dans le backend : migration `ai_decisions` (append-only, DORA), endpoint
`POST /api/ai/chat` (copilote), puis diagnostic/RCA et le cadran d'autonomie.
