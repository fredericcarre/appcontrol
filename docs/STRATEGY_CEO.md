# AppControl v4 -- Document Strategique

> **Classification :** Confidentiel -- Direction Generale
> **Date :** 24 fevrier 2026
> **Version :** 1.0
> **Audience :** CEO, Board, Investisseurs

---

## Table des matieres

1. [Synthese executive](#1-synthese-executive)
2. [Contexte du projet](#2-contexte-du-projet)
3. [Proposition de valeur unique](#3-proposition-de-valeur-unique)
4. [Les 10 capacites differenciantes](#4-les-10-capacites-differenciantes)
5. [Architecture technique](#5-architecture-technique--resume-executif)
6. [Marche cible et clients types](#6-marche-cible-et-clients-types)
7. [Modele economique (Open Core)](#7-modele-economique-open-core)
8. [Avantages competitifs vs alternatives](#8-avantages-competitifs-vs-alternatives)
9. [Roadmap 2026](#9-roadmap-2026)
10. [KPIs de succes](#10-kpis-de-succes)
11. [Risques et mitigations](#11-risques-et-mitigations)

---

## 1. Synthese executive

AppControl v4 est une plateforme d'**excellence operationnelle et de resilience IT** qui comble un vide critique dans l'ecosysteme des outils d'exploitation. Alors que les schedulers savent *quand* lancer, et que les outils de deploiement savent *quoi* deployer, **personne ne sait comment une application fonctionne** -- quels composants dependent de quels autres, dans quel ordre les demarrer, comment reagir a une panne partielle, comment basculer sur un site de secours.

AppControl est ce **chainon manquant**. Il cartographie les applications comme des graphes de dependances, monitore chaque composant en temps reel, et orchestre les operations dans l'ordre exact dicte par la topologie. Le tout avec un audit DORA natif, une securite zero-trust (mTLS partout), et une integration IA via le protocole MCP.

La v4 est une **reecriture complete en Rust** -- ~35 000 lignes de code, 19 migrations SQL, 168+ tests automatises -- livrant un binaire unique, performant et securise par construction.

---

## 2. Contexte du projet

### Pourquoi une reecriture complete ?

La version precedente d'AppControl a demontre la validite du concept aupres de clients industriels et financiers. La v4 repond a trois exigences qui ne pouvaient pas etre satisfaites par une evolution incrementale :

| Exigence | Reponse v4 |
|----------|-----------|
| **Performance** | Rust : zero-cost abstractions, pas de garbage collector, latence sub-milliseconde |
| **Securite memoire** | Garanties du compilateur Rust : pas de buffer overflow, pas de use-after-free, pas de data race |
| **Deploiement simplifie** | Binaire unique par composant, pas de runtime (JVM, Node.js, Python) a installer sur les serveurs cibles |

### Stack technologique

| Couche | Technologie | Justification |
|--------|------------|---------------|
| **Agent** | Rust + Tokio + sysinfo + nix | Binaire unique ~5 MB, zero dependance, compatible Linux/Windows |
| **Gateway** | Rust + Axum + rustls | Relais WebSocket avec mTLS natif, ~1 700 LOC |
| **Backend API** | Rust + Axum + sqlx | 75+ endpoints REST, WebSocket temps reel, ~17 600 LOC |
| **Base de donnees** | PostgreSQL 16 | Tables partitionnees, APPEND-ONLY, requetes verifiees a la compilation |
| **Frontend** | React 18 + TypeScript + Tailwind + shadcn/ui | SPA moderne, React Flow pour la cartographie |
| **CLI** | Rust (appctl) | Integration native avec schedulers existants |
| **MCP Server** | Rust | Integration IA : 10 outils pour piloter AppControl en langage naturel |

---

## 3. Proposition de valeur unique

### AppControl n'est PAS un scheduler

C'est un point de positionnement fondamental. AppControl **s'integre** avec les schedulers existants -- Control-M, AutoSys, Dollar Universe, TWS -- via REST API et CLI. Le scheduler decide *quand* ; AppControl decide *comment* et dans *quel ordre*.

### Le cockpit operationnel

AppControl est le **cockpit** depuis lequel les equipes d'exploitation visualisent, comprennent et operent leurs applications :

```
┌──────────────────────────────────────────────────────┐
│                    AppControl                         │
│              (Source de Verite Unique)                │
│                                                      │
│  Cartographie DAG  ─  Etat Temps Reel  ─  Operations│
│  Diagnostic 3 niv. ─  DR Switchover    ─  Audit DORA│
└──────────────┬───────────────┬──────────────┬────────┘
               │               │              │
    ┌──────────▼──────┐ ┌──────▼──────┐ ┌────▼────────┐
    │   Schedulers    │ │  Deploiement│ │  Monitoring  │
    │                 │ │             │ │              │
    │  Control-M      │ │  Jenkins    │ │  Datadog     │
    │  AutoSys        │ │  XL Release │ │  Prometheus  │
    │  Dollar Univ.   │ │  GitLab CI  │ │  Zabbix      │
    │  TWS            │ │  ArgoCD     │ │  Dynatrace   │
    └─────────────────┘ └─────────────┘ └──────────────┘
```

### Positionnement : le chainon manquant

AppControl se situe **entre le monitoring et l'orchestration d'infrastructure** :

- **Monitoring** (Datadog, Zabbix, Prometheus) : repond a "est-ce que ca marche ?" -- mais ne sait pas quoi faire quand ca ne marche pas, ni dans quel ordre relancer.
- **Orchestration infra** (Ansible, Terraform) : repond a "comment provisionner ?" -- mais ne connait pas les dependances applicatives au runtime.
- **AppControl** : repond a "comment fonctionne cette application, quelles sont ses dependances, et comment l'operer correctement ?"

---

## 4. Les 10 capacites differenciantes

### 1. Cartographie DAG

Chaque application est modelisee comme un **graphe oriente acyclique** (DAG). Les dependances sont explicites et typees. Le demarrage respecte l'**ordre topologique** (tri de Kahn avec parallelisme par niveau), l'arret suit l'ordre inverse. Ce n'est pas un addon -- c'est le coeur du modele de donnees.

### 2. FSM a 8 etats

Machine a etats finis rigoureuse avec transitions auditees :

```
UNKNOWN ──► STARTING ──► RUNNING ──► DEGRADED
                            │
                            ▼
              STOPPING ◄── FAILED
                 │
                 ▼
              STOPPED         UNREACHABLE
```

Chaque transition est validee par la FSM, enregistree dans `state_transitions` (APPEND-ONLY), et diffusee en temps reel via WebSocket. La distinction **FAILED** (le check a echoue) vs **UNREACHABLE** (l'agent ne repond plus) permet un diagnostic precis.

### 3. Diagnostic 3 niveaux

Un systeme de diagnostic progressif unique sur le marche :

| Niveau | Frequence | Question | Exemple |
|--------|-----------|----------|---------|
| **Level 1 -- Sante** | Toutes les 30s | Le processus est-il vivant ? | `check_cmd` : PID, port TCP, HTTP 200 |
| **Level 2 -- Integrite** | Toutes les 5min | Les donnees sont-elles coherentes ? | `integrity_check_cmd` : checksum BDD, replication OK |
| **Level 3 -- Infrastructure** | A la demande | L'OS et les prerequis sont-ils OK ? | `infra_check_cmd` : espace disque, memoire, certificats |

La **matrice de recommandation** croise les resultats des 3 niveaux pour proposer automatiquement l'action corrective : Restart simple, AppRebuild (reconstruction applicative), ou InfraRebuild (reconstruction infrastructure).

### 4. DR Switchover 6 phases

Bascule site de secours en **6 phases atomiques** avec rollback a chaque etape :

1. **PREPARE** -- Validation des prerequis, snapshot configuration
2. **STOP_SOURCE** -- Arret ordonne du site principal (DAG inverse)
3. **SYNC** -- Verification d'integrite des donnees (Level 2 checks)
4. **CONFIGURE** -- Application des overrides de configuration site cible
5. **START_TARGET** -- Demarrage ordonne du site de secours (DAG)
6. **COMMIT** -- Validation finale, mise a jour des enregistrements

Trois modes : **Full** (toute l'application), **Selective** (composants choisis), **Progressive** (par vagues).

### 5. Discovery passive (NOUVEAU)

L'agent scanne les processus en cours, les ports TCP en ecoute, et les connexions reseau etablies. Le backend **infere automatiquement** le graphe de dependances : si le processus A ecoute sur le port 5432 et le processus B a une connexion etablie vers ce port, alors B depend de A.

L'operateur revoit le draft dans un wizard visuel (React Flow), accepte/rejette/modifie les composants et les aretes, puis valide. **L'application est creee en un clic** a partir de la realite du terrain.

### 6. Estimation des temps (NOUVEAU)

Base sur l'historique des executions reelles, AppControl calcule le **P50 et P95** de chaque operation par composant. Le calcul est **DAG-aware** : pour chaque niveau de parallelisme, le temps predit est le MAX des composants de ce niveau. Le temps total est la somme des niveaux.

Resultat : avant de lancer un restart, l'operateur sait que ca prendra **~12 minutes (P50) ou ~18 minutes (P95)**. Fini les "ca devrait prendre 5 minutes" qui durent une heure.

### 7. MCP Server -- Integration IA (NOUVEAU)

Serveur MCP (Model Context Protocol) natif permettant de piloter AppControl en **langage naturel** depuis Claude Desktop, Cursor, ou tout client MCP compatible. 10 outils exposes :

| Outil | Description |
|-------|------------|
| `list_apps` | Lister toutes les applications |
| `get_app_status` | Etat temps reel d'une application |
| `start_app` / `stop_app` | Demarrer/arreter une application |
| `get_topology` | Exporter le graphe de dependances |
| `get_plan` | Simuler une operation sans l'executer |
| `run_diagnostic` | Lancer un diagnostic 3 niveaux |
| `list_agents` | Etat des agents connectes |
| `get_estimation` | Temps predit pour une operation |
| `get_discovery_draft` | Topologie decouverte automatiquement |

Un operateur peut demander a son assistant IA : *"Quel est l'etat de l'application TRADING ? Si elle est en erreur, lance un diagnostic et dis-moi quoi faire."* L'IA appelle les bons outils MCP, analyse les resultats, et formule une recommandation.

### 8. Air-gap update (NOUVEAU)

Mise a jour des agents via WebSocket **a travers le gateway**, sans aucun acces internet sur les machines cibles. Le binaire est decoupe en chunks, transfere via le canal WebSocket existant, reassemble cote agent avec verification SHA-256, puis remplace atomiquement (rename + exec).

Le backend suit la progression en temps reel (chunks recus / total). En cas d'echec partiel, le transfert reprend la ou il s'est arrete.

**Cas d'usage critique** : sites industriels, salles de marche, environnements classifies ou les serveurs n'ont pas d'acces internet.

### 9. Agent mode advisory

Deploiement **observation-only**. L'agent monitore les processus et remonte l'etat des composants, mais **refuse d'executer** toute commande de start/stop/restart (exit code -2, message explicatif). Les health checks continuent de fonctionner normalement.

Ce mode permet une **migration progressive** : deployer AppControl en mode advisory pendant 2-4 semaines, valider que le modele de dependances est correct, puis basculer en mode actif composant par composant.

### 10. Audit DORA complet

Chaque action, chaque transition d'etat, chaque commande executee est tracee dans des tables **APPEND-ONLY** :

| Table | Contenu | Politique |
|-------|---------|-----------|
| `action_log` | Toutes les actions utilisateur | INSERT uniquement, archivage (jamais supprime) |
| `state_transitions` | Chaque changement d'etat FSM | INSERT uniquement |
| `check_events` | Resultats des health checks | Partitionne par mois, retention configurable |
| `switchover_log` | Historique des bascules DR | INSERT uniquement |
| `config_versions` | Snapshots avant/apres de chaque config | INSERT uniquement |

La regle est gravee dans le code et dans les conventions du projet : **"Log before execute"** -- l'action est enregistree AVANT d'etre executee. Si le systeme plante pendant l'execution, l'audit trail existe quand meme.

7 rapports DORA natifs : disponibilite, incidents, bascules, audit, conformite, RTO, export PDF.

---

## 5. Architecture technique -- Resume executif

### Les 5 composants

```
┌─────────────────────────────────────────────────────────────┐
│                        FRONTEND                             │
│              React 18 + TypeScript + React Flow             │
│                     ~9 400 lignes                           │
└─────────────────────────┬───────────────────────────────────┘
                          │ HTTPS
┌─────────────────────────▼───────────────────────────────────┐
│                      BACKEND API                            │
│            Rust + Axum + sqlx + WebSocket                   │
│            75+ endpoints REST, ~17 600 lignes               │
│                                                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐  │
│  │ FSM      │ │ DAG      │ │ Switchov.│ │ Diagnostic    │  │
│  │ Engine   │ │ Sequencer│ │ 6 phases │ │ 3 niveaux     │  │
│  └──────────┘ └──────────┘ └──────────┘ └───────────────┘  │
└──────────┬──────────────────────────────────────────────────┘
           │ mTLS + WebSocket
┌──────────▼──────────────────────────────────────────────────┐
│                       GATEWAY                               │
│              Rust + Axum + rustls                            │
│         Relais WebSocket, ~1 700 lignes                     │
└──────────┬──────────────────────┬───────────────────────────┘
           │ mTLS + WSS           │ mTLS + WSS
┌──────────▼──────┐    ┌─────────▼────────┐
│    AGENT #1     │    │    AGENT #N      │
│  Rust + Tokio   │    │  Rust + Tokio    │
│  ~3 900 lignes  │    │  ~3 900 lignes   │
│                 │    │                  │
│ - Health checks │    │ - Health checks  │
│ - Exec commands │    │ - Exec commands  │
│ - Discovery     │    │ - Discovery      │
│ - Buffer offline│    │ - Buffer offline │
└─────────────────┘    └──────────────────┘
```

### Chiffres cles

| Metrique | Valeur |
|----------|--------|
| Lignes de code Rust | ~35 200 |
| Lignes de code Frontend (TS/TSX) | ~9 400 |
| Migrations SQL | 19 |
| Tables PostgreSQL | ~64 (dont ~10 evenementielles APPEND-ONLY) |
| Endpoints REST | 75+ |
| Tests unitaires Rust | 168+ |
| Tests frontend | 229+ (22 fichiers) |
| Tests E2E | 33 fichiers de tests |
| Specification OpenAPI | 3.0.3, complete |
| Outils MCP | 10 |

### Principes d'architecture non-negociables

1. **mTLS partout** -- Aucune communication en clair entre composants. Certificats X.509 auto-generes au demarrage.
2. **Tables evenementielles APPEND-ONLY** -- Ni UPDATE ni DELETE sur les tables d'audit. Jamais. C'est une regle du compilateur social du projet.
3. **Log before execute** -- Chaque action utilisateur est enregistree dans `action_log` AVANT l'execution.
4. **Process detachment** -- Les processus demarres par l'agent survivent au crash de l'agent (double-fork + setsid).
5. **Delta-only sync** -- L'agent n'envoie que les changements, pas l'etat complet a chaque check.
6. **Transactions FSM** -- `SELECT ... FOR UPDATE` pour prevenir les race conditions sur les transitions d'etat.

---

## 6. Marche cible et clients types

### Segment primaire : Banques et assurances

**Pourquoi ?** La reglementation DORA (Digital Operational Resilience Act) impose aux institutions financieres europeennes de :
- Cartographier leurs systemes IT critiques et leurs dependances
- Tester regulierement leur capacite de bascule DR
- Maintenir des traces d'audit completes et non-modifiables
- Demontrer des temps de reprise (RTO) maitrises

AppControl repond **nativement** a ces quatre exigences. Un concurrent comme ServiceNow peut le faire, mais a un cout 10-50x superieur et avec des mois d'integration.

**Persona type :** Directeur de la Production IT, RSSI, Responsable Conformite.

### Segment secondaire : Industrie et sites air-gap

**Pourquoi ?** Les environnements industriels (usines, centrales, sites de production) ont des contraintes specifiques :
- Pas d'acces internet sur les serveurs de production
- Mises a jour logicielles par media physique ou reseau interne uniquement
- Equipes d'exploitation reduites, pas de DevOps

La capacite d'**air-gap update** via WebSocket et le mode **advisory** pour migration progressive sont des differenciateurs uniques.

**Persona type :** Responsable informatique industrielle, DSI site.

### Segment tertiaire : MSPs (Managed Service Providers)

**Pourquoi ?** Les MSPs gerent des centaines d'applications pour le compte de leurs clients. Ils ont besoin :
- D'une vue consolidee multi-client (multi-organisation native dans AppControl)
- D'une delegation fine des permissions (5 niveaux, workspaces, equipes)
- D'un modele de facturation par application geree (s'aligne sur le modele Open Core)

**Persona type :** Directeur des Operations, Practice Manager.

### Segment de transition : Entreprises avec dette technique

**Pourquoi ?** Des milliers d'entreprises gerent encore leurs applications avec des scripts shell, des runbooks Word, et des connaissances tribales. La migration vers AppControl suit un chemin progressif en 5 phases (documenter, observer, valider, integrer, operer) avec le mode advisory comme filet de securite.

**Persona type :** Responsable d'exploitation, Chef de projet modernisation.

---

## 7. Modele economique (Open Core)

### Deux editions

| | **Community** | **Enterprise** |
|---|---|---|
| **Licence** | BSL 1.1 (→ Apache 2.0 apres 3 ans) | Licence annuelle par souscription |
| **Applications** | 5 max | Illimite |
| **Agents** | 10 max | Illimite |
| **Authentification** | JWT RS256 | SAML 2.0 + OIDC + JWT |
| **Monitoring** | Health checks (Level 1) | 3 niveaux (sante + integrite + infra) |
| **Orchestration** | Start / Stop / Restart | + DR Switchover 6 phases |
| **Discovery** | -- | Discovery passive + wizard |
| **MCP / IA** | -- | 10 outils MCP |
| **Rapports** | Disponibilite basique | 7 rapports DORA complets + export PDF |
| **Support** | Communaute (GitHub Issues) | SLA contractuel (reponse 4h/24h) |
| **Air-gap update** | -- | Inclus |
| **4-Eyes Approval** | -- | Inclus |
| **Break-Glass** | -- | Inclus |

### Principes de licensing

- **L'agent et le CLI restent Apache 2.0 dans tous les cas.** C'est la porte d'entree. Un agent gratuit deploye sur 500 serveurs cree un lock-in positif et un pipeline de conversion vers Enterprise.
- **La BSL 1.1 pour le backend** empeche les cloud providers de revendre AppControl en SaaS sans contribuer, tout en garantissant l'open-source a terme (conversion automatique apres 3 ans).
- Le modele est **previsible** pour le client : souscription annuelle, pas de facturation a l'usage.

### Pricing indicatif (a valider)

| Tier | Prix annuel | Cible |
|------|-------------|-------|
| Community | Gratuit | PME, equipes techniques, evaluation |
| Enterprise Starter | ~15 000 EUR/an | 1-20 applications, 1 site |
| Enterprise Standard | ~45 000 EUR/an | Applications illimitees, multi-site, DR |
| Enterprise Premium | ~100 000 EUR/an | + support prioritaire, SLA 4h, audit sur mesure |

---

## 8. Avantages competitifs vs alternatives

### Matrice de comparaison

| Critere | Scripts shell | Ansible | ServiceNow | BMC Helix | **AppControl** |
|---------|-------------|---------|------------|-----------|---------------|
| **Cartographie DAG** | Non | Non | Partiel (CMDB) | Partiel | **Natif** |
| **Monitoring temps reel** | Non | Non | Oui (couteux) | Oui | **Natif** |
| **Orchestration sequencee** | Manuel | Playbooks | Workflows | Workflows | **DAG automatique** |
| **DR Switchover** | Scripts adhoc | Possible | Module separe | Module separe | **6 phases integrees** |
| **Audit DORA** | Non | Non | Oui | Oui | **Natif (APPEND-ONLY)** |
| **Integration IA/MCP** | Non | Non | Non | Non | **10 outils MCP** |
| **Air-gap** | N/A | Necessite SSH | Cloud-only | Agent lourd | **WebSocket via gateway** |
| **Diagnostic 3 niveaux** | Non | Non | Non | Partiel | **Natif avec recommandations** |
| **Temps d'implementation** | Immediat (fragile) | Semaines | Mois | Mois | **Jours** |
| **Cout** | Gratuit | $$$ | $$$$ | $$$$ | **Freemium → Enterprise** |
| **Taille binaire agent** | N/A | ~100 MB (Python) | ~200 MB | ~150 MB | **~5 MB (Rust)** |

### Arguments cles par concurrent

**vs. Scripts shell :** "Vous avez deja les scripts. AppControl les orchestre dans le bon ordre, avec audit et rollback. Deployez en mode advisory, importez vos scripts en YAML, basculez quand vous etes prets."

**vs. Ansible :** "Ansible sait configurer. AppControl sait operer. Ansible ne connait pas l'etat actuel de vos composants. AppControl si. Les deux sont complementaires."

**vs. ServiceNow :** "ServiceNow couvre 100 cas d'usage a 80%. AppControl couvre 1 cas d'usage a 100% : les operations applicatives. Et il coute 10x moins cher."

---

## 9. Roadmap 2026

### Q1 (Janvier - Mars) : Stabilisation et beta privee

| Jalon | Statut | Detail |
|-------|--------|--------|
| Codebase v4 complete | Fait | 12 phases implementees, toutes validees |
| Documentation technique | Fait | QUICKSTART, USER_GUIDE, POSITIONING, INTEGRATION_COOKBOOK, MIGRATION, SECURITY_ARCHITECTURE |
| Specification OpenAPI | Fait | 75+ endpoints documentes |
| 3 POC clients pilotes | En cours | Identification et engagement |
| Retours beta integres | A faire | Boucle de feedback structuree |

### Q2 (Avril - Juin) : Lancement open-source

| Jalon | Date cible | Detail |
|-------|-----------|--------|
| Publication GitHub | Avril 2026 | Repository public, licence BSL 1.1 + Apache 2.0 (agent/CLI) |
| Documentation communautaire | Avril 2026 | README, CONTRIBUTING, guides d'installation |
| Site web produit | Mai 2026 | Landing page, documentation en ligne, demo interactive |
| Community building | Continu | Blog posts, talks (DevOps meetups, Devoxx), Hacker News launch |
| 100 stars GitHub | Juin 2026 | Objectif de traction |

### Q3 (Juillet - Septembre) : GA Enterprise

| Jalon | Date cible | Detail |
|-------|-----------|--------|
| GA Enterprise | Juillet 2026 | Version Enterprise officielle, support SLA |
| Premier client payant | Aout 2026 | Conversion d'un POC Q1 |
| Certification SOC 2 Type II | En cours Q3 | Processus demarre en Q2 |
| SaaS pilot interne | Septembre 2026 | Infrastructure multi-tenant pour evaluer le modele SaaS |

### Q4 (Octobre - Decembre) : Marketplace et SaaS

| Jalon | Date cible | Detail |
|-------|-----------|--------|
| Marketplace connecteurs | Octobre 2026 | ServiceNow, Jira, PagerDuty, Slack |
| SaaS option beta | Novembre 2026 | Offre hebergee pour les clients qui ne veulent pas on-premise |
| 5 PRs communautaires | Decembre 2026 | Preuve de l'adoption communautaire |
| Bilan annuel | Decembre 2026 | KPIs, pipeline, decisions strategiques 2027 |

---

## 10. KPIs de succes

### Traction (metriques open-source)

| KPI | Cible Q2 | Cible Q4 | Methode de mesure |
|-----|----------|----------|-------------------|
| Stars GitHub | 100 | 500 | GitHub API |
| Forks | 15 | 50 | GitHub API |
| Downloads agent (binaire) | 500 | 2 000 | GitHub Releases / compteur |
| PRs externes | 2 | 5 | GitHub PRs (hors equipe) |
| Issues communautaires | 20 | 100 | GitHub Issues |

### Commercial (metriques business)

| KPI | Cible Q1 | Cible Q3 | Cible Q4 |
|-----|----------|----------|----------|
| POC clients | 3 | 3 actifs | 5 actifs |
| Clients Enterprise payants | 0 | 1 | 3 |
| ARR (revenu annuel recurrent) | 0 | 45 000 EUR | 150 000 EUR |
| Pipeline qualifie | 5 prospects | 10 prospects | 20 prospects |

### Produit (metriques qualite)

| KPI | Actuel | Cible Q4 |
|-----|--------|----------|
| Tests automatises (Rust) | 168+ | 300+ |
| Tests frontend | 229+ | 400+ |
| Couverture de code | ~60% | 80%+ |
| Temps de build CI | < 10 min | < 8 min |
| Zero CVE critique | Oui (Trivy) | Maintenu |

---

## 11. Risques et mitigations

| # | Risque | Impact | Probabilite | Mitigation |
|---|--------|--------|------------|------------|
| R1 | **Adoption lente** -- le marche ne comprend pas le positionnement | Eleve | Moyenne | Contenu educatif (blog, talks), POC gratuits, mode advisory comme porte d'entree |
| R2 | **Concurrence ServiceNow/BMC** -- les grands editeurs ajoutent des fonctionnalites similaires | Moyen | Faible | Vitesse d'execution, communaute open-source, prix 10x inferieur |
| R3 | **Complexite du go-to-market** -- vendre un outil d'exploitation IT est difficile (acheteur != utilisateur) | Eleve | Moyenne | Bottom-up adoption (agent gratuit), puis conversion top-down (Enterprise) |
| R4 | **Dette technique Rust** -- difficulte de recrutement de developpeurs Rust | Moyen | Moyenne | Documentation exhaustive (CLAUDE.md par crate), CI strict, Rust monte en popularite |
| R5 | **Dependance a un seul developpeur** | Critique | Elevee court terme | Open-source + documentation + CI autofix reduisent le bus factor progressivement |
| R6 | **Reglementation DORA retardee ou assouplie** | Moyen | Faible | La valeur operationnelle existe independamment de DORA (gain de temps, reduction des incidents) |

---

## Annexes

### A. Glossaire

| Terme | Definition |
|-------|-----------|
| **DAG** | Directed Acyclic Graph -- graphe oriente sans cycle, modelisant les dependances entre composants |
| **FSM** | Finite State Machine -- machine a etats finis gerant le cycle de vie de chaque composant |
| **mTLS** | Mutual TLS -- authentification bidirectionnelle par certificats X.509 |
| **DORA** | Digital Operational Resilience Act -- reglementation europeenne sur la resilience operationnelle numerique |
| **MCP** | Model Context Protocol -- protocole d'integration IA (Anthropic) |
| **BSL** | Business Source License -- licence open-source avec restriction commerciale temporaire |
| **RTO** | Recovery Time Objective -- temps maximal de reprise apres incident |
| **P50/P95** | Percentiles statistiques (mediane et 95e percentile) |

### B. References documentaires

| Document | Emplacement | Audience |
|----------|------------|----------|
| Guide de demarrage rapide | `docs/QUICKSTART.md` | Developpeurs, evaluateurs |
| Guide utilisateur complet | `docs/USER_GUIDE.md` | Operateurs, administrateurs |
| Architecture securite | `SECURITY_ARCHITECTURE.md` | RSSI, auditeurs |
| Positionnement produit | `docs/POSITIONING.md` | Commercial, partenaires |
| Cookbook d'integration | `docs/INTEGRATION_COOKBOOK.md` | Equipes integration |
| Migration depuis scripts | `docs/MIGRATION_FROM_SCRIPTS.md` | Prospects en evaluation |
| Specification OpenAPI | `/api/v1/openapi.json` (runtime) | Developpeurs API |

---

*Document genere le 24 fevrier 2026. AppControl v4 -- Tous droits reserves.*
