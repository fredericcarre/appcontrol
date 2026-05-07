# AppControl

**Vos outils vous montrent l'infrastructure. AppControl vous montre l'application.**

Vos pods peuvent tourner pendant que votre application est cassée. Vos batchs peuvent se lancer pendant qu'une dépendance est tombée. Vos écrans de supervision peuvent être tout verts pendant qu'un client ne peut pas payer.

Schedulers, supervisions, Kubernetes, runbooks Word, scripts éparpillés — chaque équipe a son outil, chacun voit un morceau. **Personne ne voit l'application.** AppControl ne remplace rien. Il pose la couche applicative au-dessus de tout ça : la vue d'ensemble exécutable qui vous manque.

> *La promenade en prod la plus simple que vous ayez jamais faite.*

[![CI](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml)
[![codecov](https://codecov.io/gh/fredericcarre/appcontrol/graph/badge.svg)](https://codecov.io/gh/fredericcarre/appcontrol)
[![Release](https://img.shields.io/github/v/release/fredericcarre/appcontrol?display_name=tag&sort=semver)](https://github.com/fredericcarre/appcontrol/releases/latest)
[![License: Proprietary](https://img.shields.io/badge/license-Proprietary-red.svg)](#license)

---

> *« C'est dangereux. Mais pourquoi pas. »*
> — Directeur de Production, banque française tier-1

> *« Si j'avais ça, j'aurais besoin de 70 % d'ingénieurs de production en moins. »*
> — Même interlocuteur, dans la même réunion

---

## Trois moments, trois clics

### Dimanche 3h17 — le batch core banking a planté

Votre senior sysadmin est en vacances. La doc d'exploitation a deux ans de retard.

Vous ouvrez AppControl. La carte de l'application est déjà à l'écran. La branche en erreur est en rouge.
Un clic sur **Restart error branch**. Les composants redémarrent dans le bon ordre, en parallèle quand c'est possible.
Quatre minutes plus tard, tout est vert. L'audit signé est dans votre boîte mail.

<!-- SCREENSHOT:incident-recovery -->

### Mardi 14h — exercice de bascule DR Paris → Lyon

Six phases, rollback possible à chaque étape. Vous voyez chaque composant changer de site en temps réel. Le rapport de conformité est prêt avant la fin de la réunion.

<!-- SCREENSHOT:dr-switchover -->

### Vendredi 10h — l'ACPR demande la trace de la dernière bascule

Un clic sur **Export DORA**. Tout est là : signé, daté, immuable, append-only. Qui a fait quoi, quand, et pourquoi.

<!-- SCREENSHOT:audit-export -->

---

## Le quatrième moment : pilotez votre prod en langage naturel

AppControl expose un **serveur MCP natif**. Connectez Claude, ChatGPT, Cursor ou n'importe quel client compatible — et parlez à votre production.

```
Vous : "Quelles applications sont en état dégradé ce matin ?"
Claude : Trois applications : core-banking (branche batch en erreur),
         payment-gateway (latence anormale), reporting (composant absent).

Vous : "Redémarre la branche en erreur sur core-banking, en mode dry-run d'abord."
Claude : Plan d'exécution : 3 composants à redémarrer dans l'ordre
         [batch-loader → reconciler → reporter]. Durée estimée : 2 min 40.
         Je lance ?
```

<!-- SCREENSHOT:mcp-claude-control -->

C'est la première plateforme d'exploitation **AI-native** pour applications critiques. Aucun outil ITOM existant ne le permet aujourd'hui.

---

## Pourquoi maintenant

**DORA est en vigueur. NIS2 aussi.** Vos régulateurs vous demandent une chose simple :

> *« Prouvez-moi, à la seconde près, que vous savez ce qui tourne, dans quel ordre, et que vous pouvez le redémarrer sous contrôle. »*

Aucun de vos outils actuels ne sait faire ça seul. AppControl le fait à leur place, **sans rien remplacer**.

**Et la souveraineté redevient un sujet de comité.** Vos outils d'exploitation critiques sont presque tous américains : Datadog, Dynatrace, ServiceNow, BMC, PagerDuty. Toutes vos opérations passent par eux — leurs SLA, leurs juridictions, leurs lois extraterritoriales. AppControl se déploie **on-prem, en cloud privé, ou en air-gap complet**. Code Rust auditable, binaires signés, aucune dépendance à un cloud étranger. À notre connaissance, c'est aujourd'hui la seule plateforme d'exploitation applicative souveraine, sur ce niveau de profondeur fonctionnelle.

---

## L'angle mort de Kubernetes

Kubernetes orchestre vos conteneurs. Il vous dit si les pods tournent. **Il ne vous dit jamais si votre application fonctionne.**

Il ne vous parle pas non plus des 70 % de votre SI qui ne tourneront jamais en Kube : mainframe, AS/400, batchs Cobol, monolithes Oracle, services Windows. Et il vous demande des compétences profondes, des configurations énormes, des équipes dédiées — pour finalement vous montrer… des conteneurs.

**AppControl orchestre vos applications. Y compris au-dessus de Kube.** C'est la couche que personne ne fournit aujourd'hui.

| AppControl n'est pas… | Il s'intègre avec |
|---|---|
| Un scheduler | Control-M, AutoSys, $Universe, TWS |
| Un outil de supervision | Datadog, Dynatrace, Prometheus, Zabbix |
| Un orchestrateur de conteneurs | Kubernetes, OpenShift |
| Une CMDB | ServiceNow, BMC |
| Un outil de déploiement | XL Release, Jenkins, GitLab CI, ArgoCD |

---

## Pour qui

- **Banques, assurances, paiements** — DORA, continuité d'activité, audits ACPR/AMF
- **Télécoms, énergie, transport, santé** — NIS2, criticité 24/7
- **Intégrateurs et MSP** — exploitation déléguée d'applications critiques

---

## Démarrer

Pas de formulaire. Pas de POC bricolé. Le bon premier pas, c'est un **engagement cadré** sur un périmètre nommé : une application critique, ou un projet de reconstruction en cours.

📩 **Décrivez votre cas en trois lignes** : un nom d'application, le scheduler en place, l'horizon DR à couvrir.
Réponse sous 48h.

📞 [Prendre 15 minutes en visio](#) pour une démonstration adaptée à votre stack.

---

## Sous le capot — *dossier technique*

Pile entièrement open architecture, déployable on-prem, en cloud privé, ou en mode air-gap.

| Couche | Technologie |
|---|---|
| Agents | Rust 1.88+ · Tokio · sysinfo · détachement de processus, buffer offline, mTLS |
| Gateway | Rust · Axum 0.7 · rustls · relais WebSocket |
| Backend | Rust · Axum · sqlx · PostgreSQL 16 ou SQLite · journal d'audit append-only |
| Frontend | React 18 · TypeScript 5.3 · Vite 5 · Tailwind · shadcn/ui · React Flow |
| MCP | Crate Rust dédié, exposé via stdio ou HTTP |
| Authentification | OIDC · SAML 2.0 · JWT RS256 · RBAC à 5 niveaux · partage par lien |
| Déploiement | Docker · Helm · OpenShift · mode air-gap |

### Démarrage en 5 minutes

```bash
git clone https://github.com/fredericcarre/appcontrol.git && cd appcontrol
docker compose -f docker/docker-compose.release.yaml up -d
open http://localhost:8080
```

Connexion : `admin@localhost`, mot de passe vide.

### CLI

```bash
appctl start core-banking --wait --timeout 120
appctl status core-banking --format table
appctl diagnose core-banking --level 2
appctl switchover core-banking --target-site lyon --mode FULL --wait
```

### Cartes d'application prêtes à l'emploi

Trois exemples dans [`examples/`](examples/) :

| Exemple | Composants | Points clés |
|---|:---:|---|
| [Three-Tier Web App](examples/three-tier-webapp.json) | 7 | Dépendances fortes/faibles, réplication BDD, batch |
| [Microservices E-Commerce](examples/microservices-ecommerce.json) | 12 | API gateway, message broker, service-per-DB |
| [Core Banking System](examples/banking-core-system.json) | 9 | Bascule DR Paris→Lyon, intégration Control-M, conformité DORA |

### Pour aller plus loin

- [QUICKSTART](docs/QUICKSTART.md) — installation complète, agents, premier pilote
- [Architecture](docs/architecture.md) — composants, flux, FSM, séquencement DAG
- [Sécurité](SECURITY_ARCHITECTURE.md) — mTLS, signature des audits, modèle de menace
- [Positionnement](docs/POSITIONING.md) — où AppControl s'insère dans votre écosystème
- [Conformité DORA](docs/PERMISSIONS.md) — RBAC, traçabilité, exports régulateur
- [Déploiement OpenShift](docs/OPENSHIFT.md) · [Azure Gateway](docs/AZURE_GATEWAY.md) · [Windows](docs/WINDOWS_DEPLOYMENT.md)

### Couverture de tests

| Module | Cible | Périmètre |
|---|:---:|---|
| `common/` | 90% | Transitions FSM, sérialisation protocole |
| `backend/core/` | 80% | FSM, DAG, permissions, switchover, diagnostics |
| `backend/api/` | 70% | Tous les endpoints : happy path + erreurs |
| `agent/` | 75% | Exécuteur, scheduler, buffer offline |
| `frontend/` | 60% | Hooks, stores, logique de permission |
| **E2E** | 9 scénarios | Pile complète avec base réelle et WebSocket |

```bash
cargo llvm-cov --workspace --html --output-dir coverage/
cd frontend && npm test -- --coverage
```

Voir [COVERAGE.md](COVERAGE.md).

### Développement

```bash
docker compose -f docker/docker-compose.dev.yaml up -d
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cd frontend && npm run lint && npm run build
```

Voir [PROGRESS.md](PROGRESS.md) et le `CLAUDE.md` du crate concerné.

---

## License

Propriétaire. Tous droits réservés.
