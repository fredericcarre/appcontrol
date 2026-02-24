# AppControl et la Cyber-Resilience : Reconstruire en heures, pas en semaines

> **Document confidentiel** -- Positioning paper pour Linedata
>
> Contexte : En situation de cyberattaque, la restauration des donnees n'est pas le goulet d'etranglement. C'est la reconstruction de la topologie applicative qui prend des semaines. AppControl elimine ce risque.

---

## Le probleme : 3 semaines pour reconstruire

Quand une cyberattaque frappe une entreprise comme Linedata, le scenario est toujours le meme :

1. **Les equipes securite isolent le SI** -- c'est rapide (heures)
2. **Les backups sont disponibles** -- les donnees sont la (heures a jours)
3. **La reconstruction applicative commence** -- et c'est la que tout s'effondre (**semaines**)

### Pourquoi 3 semaines ?

Le probleme n'est pas technique au sens strict. C'est un probleme de **connaissance operationnelle** :

- **Quel processus tourne sur quelle machine ?** Les noms de serveurs ont change, les FQDN ne correspondent plus, les adresses IP ont ete reassignees.
- **Dans quel ordre demarrer ?** Le serveur de trading depend de la base de marche, qui depend du bus de messages, qui depend du DNS interne. Personne n'a cette sequence complete en tete.
- **Quelles sont les dependances cachees ?** Le batch de rapprochement qui echoue silencieusement parce que le service de pricing n'est pas encore monte -- ca, c'est 2 jours de debug.
- **Comment verifier que tout fonctionne ?** Un processus qui tourne ne signifie pas qu'il fonctionne. Il faut verifier la sante, l'integrite des donnees, les prerequis infrastructure.

### Ou est cette connaissance aujourd'hui ?

| Source | Fiabilite | Disponibilite post-attaque |
|--------|-----------|---------------------------|
| Runbooks Word/Confluence | Obsoletes dans 80% des cas | Si le serveur Confluence est restaure... |
| Scripts shell maison | Partiellement a jour | Si le partage NFS est restaure... |
| La tete des experts | Precise mais incomplete | Si l'expert est disponible a 3h du matin pendant 3 semaines... |
| Ordonnanceur (Control-M, etc.) | Partiellement, enchainements seulement | Si l'ordonnanceur est restaure en premier... |

**Resultat : 3 semaines de tatonnement, de tests manuels, de decouverte empirique.** On redemarre un service, il echoue. On cherche pourquoi. On decouvre une dependance oubliee. On redemarre la dependance. Elle echoue aussi. Et ainsi de suite, sur des centaines de composants.

---

## La solution AppControl : de 3 semaines a quelques heures

AppControl adresse ce probleme en trois phases distinctes.

### Phase 1 : Cartographie pre-attaque (temps normal)

AppControl cartographie **l'integralite du parc applicatif** sous forme de graphes de dependances (DAG -- Directed Acyclic Graphs). Cette cartographie se fait **en continu, en silence, sans aucun impact sur la production**.

#### Ce qu'AppControl cartographie pour chaque composant :

| Information | Exemple |
|-------------|---------|
| **Identite** | `trading-engine` sur `srv-trading-01.linedata.net` |
| **Agent hote** | Agent AppControl sur la machine, avec son FQDN et ses adresses IP |
| **Commande de demarrage** | `/opt/trading/bin/start.sh --mode=production` |
| **Commande d'arret** | `/opt/trading/bin/stop.sh --graceful` |
| **Health check (Niveau 1)** | `curl -s http://localhost:8080/health` -- toutes les 30 secondes |
| **Verification d'integrite (Niveau 2)** | `check_data_consistency.sh` -- toutes les 5 minutes |
| **Verification infrastructure (Niveau 3)** | `check_disk_space.sh && check_ntp_sync.sh` -- a la demande |
| **Dependances** | Depend de `market-data-feed`, `message-bus`, `ref-data-cache` |
| **Groupe fonctionnel** | "Trading Front-Office" |
| **Liens** | URL du runbook, lien CMDB, dashboard Grafana |

#### Deux modes de cartographie :

**1. Discovery passive (automatique)**

L'agent AppControl scanne la machine en continu :
- Enumere les processus actifs, les ports en ecoute, les connexions TCP etablies
- Le backend correle les rapports de tous les agents pour inferer les dependances : si le processus A sur `srv-01` a une connexion vers le port 5432 de `srv-02`, ou tourne PostgreSQL, alors A depend de la base de donnees
- Resultat : un brouillon de topologie que l'operateur valide via une interface visuelle (React Flow)

**2. Import de topologies existantes**

AppControl importe directement les fichiers YAML des anciens runbooks ou des configurations existantes, transformant automatiquement les formats legacy en modele DAG v4.

#### Resultat : un "blueprint" complet du SI, toujours a jour

Chaque modification est tracee dans `config_versions` avec un diff avant/apres en JSONB. Le blueprint n'est jamais obsolete car il est nourri en temps reel par les agents.

---

### Phase 2 : Post-attaque -- Reconstruction sequencee

Le jour J arrive. L'attaque est contenue. Les machines sont reinstallees sur des OS propres. Les backups sont restaures. **C'est maintenant que la difference se fait.**

#### Etape 1 : Redeploiement des agents (30 minutes)

- L'agent AppControl est un **binaire unique de moins de 5 MB** (compile en Rust, zero dependance externe)
- Pas besoin de Java, Python, Node.js, ni d'aucun runtime
- **Air-gap compatible** : en cas de reseau compromis, l'agent peut etre deploye par **cle USB** ou tout autre media physique
- Configuration minimale : un fichier YAML avec l'URL du gateway (ou directement du backend)
- Multi-gateway : si le gateway primaire est injoignable, l'agent bascule automatiquement sur les gateways secondaires (failover integre, avec backoff exponentiel et reessai periodique vers le primaire)
- **Mise a jour a distance** : le backend peut pousser une mise a jour de l'agent directement via WebSocket, par fragments (chunked transfer), avec verification SHA-256 -- aucun acces internet requis

#### Etape 2 : Reconstruction DAG topologique (4-8 heures)

AppControl envoie l'ordre de demarrage **exact**, niveau par niveau :

```
Niveau 0 (infrastructure) :
  ├── dns-internal          [demarrage → health check OK ✓]
  ├── ntp-server            [demarrage → health check OK ✓]
  └── certificate-authority [demarrage → health check OK ✓]

Niveau 1 (middleware) :
  ├── postgresql-primary    [demarrage → health check OK ✓]
  ├── message-bus           [demarrage → health check OK ✓]
  └── redis-cache           [demarrage → health check OK ✓]

Niveau 2 (services metier) :
  ├── market-data-feed      [demarrage → health check OK ✓]
  ├── ref-data-service      [demarrage → health check OK ✓]
  └── pricing-engine        [demarrage → health check OK ✓]

Niveau 3 (applications front) :
  ├── trading-engine        [demarrage → health check OK ✓]
  ├── risk-calculator       [demarrage → health check OK ✓]
  └── reporting-batch       [demarrage → health check OK ✓]

Niveau 4 (portails) :
  ├── client-portal         [demarrage → health check OK ✓]
  └── admin-dashboard       [demarrage → health check OK ✓]
```

**Principes cles :**
- Les composants d'un meme niveau demarrent **en parallele** (tokio async, pas de sequencement artificiel)
- On ne passe au niveau suivant que lorsque **tous les composants du niveau courant sont RUNNING** (valide par health check)
- Si un composant echoue, AppControl **suspend** (pas d'annulation en cascade) et alerte l'operateur pour decision
- Le "smart start" **ignore les composants deja running**, ne redemarre que ce qui est necessaire
- La detection de "branche rose" (pink branch) identifie automatiquement la sous-arborescence impactee par un composant en echec, pour ne redemarrer que le strict necessaire

#### Etape 3 : Estimation du temps en temps reel

Pendant la reconstruction, AppControl affiche :

- **Temps prevu par composant** : base sur l'historique reel des demarrages precedents (P50 et P95)
- **Temps prevu par niveau** : MAX(P95) des composants du niveau (puisqu'ils demarrent en parallele)
- **Temps total estime** : somme des temps par niveau = duree totale de la reconstruction
- **Progression en direct** : via WebSocket, chaque changement d'etat est pousse en temps reel vers l'interface

Le COMEX pose la question : *"Combien de temps avant qu'on soit operationnel ?"*
AppControl donne la reponse : *"47 minutes pour Trading, 2h12 pour l'ensemble du SI, base sur les P95 historiques."*

---

### Phase 3 : Validation -- Diagnostic 3 niveaux

Une fois les composants demarres, un processus qui tourne ne signifie pas qu'il fonctionne correctement. AppControl execute un diagnostic systematique sur 3 niveaux :

| Niveau | Question | Commande | Frequence |
|--------|----------|----------|-----------|
| **1 - Sante** | Le processus est-il vivant et reactif ? | `check_cmd` | Toutes les 30s (continu) |
| **2 - Integrite** | Les donnees sont-elles coherentes ? | `integrity_check_cmd` | Toutes les 5min ou a la demande |
| **3 - Infrastructure** | L'OS, le disque, les prerequis sont-ils OK ? | `infra_check_cmd` | A la demande |

#### Matrice de decision automatique

AppControl croise les trois niveaux et produit une recommandation actionnnable :

| Sante | Integrite | Infra | Recommandation |
|-------|-----------|-------|----------------|
| OK | OK | OK | **Sain** -- rien a faire |
| OK | OK | FAIL | **Sain** (alerte infra pour anticipation) |
| FAIL | OK | OK | **Restart** -- simple redemarrage suffit |
| FAIL | FAIL | OK | **Rebuild applicatif** -- reinstallation applicative necessaire |
| FAIL | * | FAIL | **Rebuild infrastructure** -- la machine elle-meme a un probleme |
| N/A | N/A | N/A | **Inconnu** -- agent non joignable |

**En situation post-attaque**, cette matrice est critique : elle permet de trier instantanement les centaines de composants entre ceux qui necessitent un simple restart, ceux qui necessitent un rebuild applicatif, et ceux ou l'infrastructure elle-meme doit etre reprise.

#### Protection et orchestration du rebuild

- Les composants critiques peuvent etre marques **"rebuild-protected"** pour empecher toute action automatique
- Le rebuild s'execute dans l'ordre DAG (les dependances d'abord)
- Pour les rebuilds infrastructure, AppControl utilise un **agent bastion** (machine tierce de confiance) pour commander la reconstruction
- Chaque action est tracee dans l'audit trail avec horodatage

---

## Pourquoi AppControl est unique pour ce use case

### 1. Le DAG EST le plan de reconstruction

Sans DAG, vous reconstruisez a l'aveugle. Avec AppControl, vous avez **le plan exact** : quoi, ou, dans quel ordre, avec quelles commandes, et quelles verifications. Ce n'est pas un document Word qui vieillit -- c'est un modele vivant, nourri en continu par les agents.

L'API d'export de topologie permet a tout moment d'extraire ce plan en JSON, YAML ou Graphviz DOT :
```
GET /api/v1/apps/{id}/topology?format=json
```
Resultat : composants, dependances, ordre de demarrage, ordre d'arret, groupes par niveaux paralleles.

### 2. Le mode advisory = zero risque en temps normal

Le principal frein a l'adoption d'un nouvel outil en production est la peur de l'impact. AppControl repond a cela avec le **mode advisory** :

- L'agent observe sans jamais executer de commande operationnelle (start/stop/rebuild)
- Les health checks s'executent normalement (lecture seule, aucune modification)
- La cartographie se construit en silence
- Les dependances sont inferees par la discovery passive

**Zero impact sur la production. La cartographie se fait sans que personne ne le remarque.**

Le jour ou l'attaque frappe, le modele est pret. On bascule les agents en mode actif et la reconstruction peut commencer.

### 3. Air-gap compatible

Post-attaque, le reseau est potentiellement compromis. Les outils cloud-dependent sont inutilisables. AppControl est concu pour fonctionner en environnement deconnecte :

- **Agent autonome** : binaire statique Rust, aucune dependance externe, deploiement par cle USB
- **Gateway isole** : communication agent-backend via WebSocket dedie, sans acces internet
- **Mise a jour par WebSocket** : le backend pousse les mises a jour d'agent par fragments via le canal WebSocket existant, avec verification SHA-256 de chaque binaire
- **Buffer offline** : si un agent perd la connexion, il stocke localement (sled, 100 MB FIFO) et rejoue automatiquement a la reconnexion -- aucune perte de donnees

### 4. Estimation des temps basee sur des donnees reelles

En situation de crise, la question numero un du COMEX est : **"Quand est-ce qu'on repart ?"**

Avec des runbooks Word, la reponse est : "On ne sait pas."

Avec AppControl, la reponse est precise et documentee :
```
GET /api/v1/apps/{id}/estimates?operation=start

{
  "operation": "start",
  "total_estimated_p50_seconds": 1847,
  "total_estimated_p95_seconds": 2832,
  "levels": [
    { "level": 0, "components": ["dns", "ntp"], "p95_seconds": 12 },
    { "level": 1, "components": ["postgresql", "rabbitmq"], "p95_seconds": 45 },
    { "level": 2, "components": ["market-data", "pricing"], "p95_seconds": 120 },
    ...
  ]
}
```

Ces estimations sont basees sur l'**historique reel** des demarrages, pas sur des estimations humaines optimistes.

### 5. IA integree (serveur MCP)

AppControl integre un serveur MCP (Model Context Protocol) compatible Claude Desktop et autres assistants IA. En situation de crise, ou les equipes sont sous pression et fatiguees, l'operateur peut interagir en langage naturel :

| Commande naturelle | Outil MCP | Action |
|-------------------|-----------|--------|
| "Quel est l'etat de Trading ?" | `get_app_status` | Retourne l'etat de chaque composant |
| "Combien de temps pour demarrer Billing ?" | `estimate_time` | Retourne P50/P95 depuis l'historique |
| "Affiche la topologie de Risk" | `get_topology` | Retourne le DAG complet |
| "Demarre Trading en dry-run" | `start_app(dry_run=true)` | Simule sans executer |
| "Diagnostique toutes les applications" | `diagnose_app` | Lance le diagnostic 3 niveaux |
| "Quels incidents cette semaine ?" | `get_incidents` | Retourne l'historique des incidents |

**Cela reduit le besoin d'expertise specifique AppControl en situation de crise** : n'importe quel operateur forme peut piloter la reconstruction avec l'assistance de l'IA.

### 6. Audit trail complet (conformite DORA)

Chaque action est enregistree **avant son execution** (Critical Rule #3) dans des tables **append-only** (Critical Rule #2) :

- `action_log` : qui a fait quoi, quand, sur quel composant, avec quel resultat
- `state_transitions` : chaque changement d'etat avec l'etat precedent et la cause
- `check_events` : chaque resultat de health check (partitionne par mois pour performance)
- `switchover_log` : chaque phase de basculement DR

Ces traces sont **inalterable** -- pas d'UPDATE, pas de DELETE, jamais. C'est exactement ce que demande DORA Article 11 et ce qu'attend le regulateur apres un incident cyber.

---

## Chiffrage du ROI

### Cout d'une reconstruction manuelle (scenario Linedata)

| Poste | Calcul | Montant |
|-------|--------|---------|
| Main d'oeuvre | 3 semaines x 15 experts x 800 EUR/jour | **180 000 EUR** |
| Perte de CA directe | ~5 MEUR/semaine x 3 semaines (estimation conservative pour une societe cotee) | **15 MEUR** |
| Penalites SLA clients | Engagements contractuels non respectes | **500 KEUR - 2 MEUR** (variable) |
| Impact reputationnel | Cours de bourse, confiance clients, due diligence prospects | **Incalculable mais significatif** |
| Cout regulatoire | Non-conformite DORA, notification ANSSI, audits supplementaires | **200 KEUR - 1 MEUR** |
| **TOTAL** | | **~16 - 18 MEUR** |

### Avec AppControl

| Poste | Amelioration |
|-------|-------------|
| Temps de reconstruction | **4-8 heures** au lieu de 3 semaines |
| Experts mobilises | 2-3 operateurs guides par AppControl au lieu de 15 experts en tatonnement |
| Validation | Diagnostic automatique 3 niveaux au lieu de tests manuels un par un |
| Audit trail | Genere automatiquement, conforme DORA, pret pour le regulateur |
| Reduction du cout total | **~95%** |
| CA preserve | L'essentiel des 15 MEUR de CA hebdomadaire est preserve |

### Investissement AppControl

| Poste | Cout |
|-------|------|
| Licence Enterprise (SI de taille moyenne) | ~50 KEUR/an |
| Mise en place initiale + accompagnement | ~30 KEUR (one-shot) |
| **Total premiere annee** | **~80 KEUR** |

### ROI

- **Des le premier incident** : 80 KEUR investis vs 16 MEUR+ de pertes evitees = **ROI x200**
- **Meme sans incident** : la cartographie continue ameliore les operations quotidiennes (demarrages/arrets coordonnes, gestion DR, conformite DORA)
- **Assurabilite** : les assureurs cyber valorisent de plus en plus la capacite de reconstruction demontrable. AppControl peut reduire les primes.

---

## Proposition concrete pour Linedata

### Phase 1 : POC advisory (2 semaines)

| Semaine | Action |
|---------|--------|
| S1 | Installation des agents en **mode advisory** sur 3-5 applications critiques (Trading, Billing, Risk). Zero impact production. |
| S1 | Configuration des health checks (niveau 1) pour chaque composant. |
| S2 | Lancement de la **discovery passive** : AppControl scanne et infere automatiquement les dependances. |
| S2 | Revue conjointe du brouillon de topologie : les equipes Linedata valident/corrigent les dependances inferees. |

### Phase 2 : Modelisation complete (1 semaine)

| Action | Detail |
|--------|--------|
| Finalisation du DAG | Import des topologies existantes (YAML, documentation) + ajustements manuels |
| Configuration des 3 niveaux de diagnostic | Health checks, integrite, infrastructure pour chaque composant |
| Premiers historiques | AppControl commence a collecter les durees de demarrage reelles |

### Phase 3 : Simulation DR (1 jour)

| Etape | Detail |
|-------|--------|
| Dry-run complet | `GET /api/v1/apps/{id}/plan?operation=start` -- AppControl calcule le plan sans executer |
| Validation de sequence | `POST /api/v1/apps/{id}/validate-sequence` -- on verifie que l'ordre propose est correct |
| Estimation | `GET /api/v1/apps/{id}/estimates?operation=start` -- temps prevu base sur l'historique |

### Resultat tangible

A l'issue du POC (3 semaines + 1 jour), Linedata dispose de :

> **"Votre application Trading peut etre reconstruite en 47 minutes au lieu de 3 jours."**
>
> **"L'ensemble de votre SI critique (Trading + Billing + Risk) peut etre reconstruit en 4h12, ordonne en 7 niveaux de dependances, avec verification automatique a chaque etape."**
>
> **"Chaque reconstruction est tracee dans un audit trail conforme DORA, pret pour le regulateur."**

---

## Annexe : references reglementaires

### DORA (Digital Operational Resilience Act) -- Reglement UE 2022/2554

- **Article 11** : *Les entites financieres elaborent et documentent des politiques et des plans de continuite des activites ICT [...] qu'elles testent au moins une fois par an.*
- **Article 12** : *Les entites financieres mettent en place des mecanismes permettant de detecter rapidement les activites anormales [...] et definissent des procedures de reponse et de reprise.*
- **Article 25** : *Les entites financieres declarent les incidents majeurs lies aux TIC a l'autorite competente [...] y compris la duree de l'indisponibilite et les mesures correctives.*

AppControl adresse directement ces trois articles : plans de continuite documentes et testes (Art. 11), detection et reprise structuree (Art. 12), audit trail complet pour la declaration d'incidents (Art. 25).

### ANSSI (France)

- **Guide d'hygiene informatique, Mesure 40** : *"Disposer d'un plan de reprise d'activite informatique formellement defini, a jour et regulierement teste"*
- **Recommandations pour la reconstruction d'un SI compromis** (CERTFR-2024) : *"Documenter l'ensemble des dependances applicatives avant tout incident"*, *"Disposer d'un inventaire a jour des composants du SI"*

### ISO 22301 (Business Continuity Management)

- **Clause 8.4** : *"L'organisme doit etablir, mettre en oeuvre et maintenir des plans de continuite d'activite et des procedures de reprise"*
- **Clause 8.5** : *"L'organisme doit realiser des exercices et des tests a intervalles planifies"*

### NIS2 (Directive UE 2022/2555)

- **Article 21** : *"Les entites essentielles et importantes prennent des mesures techniques, operationnelles et organisationnelles [...] comprenant [...] la continuite des activites, comme la gestion des sauvegardes et la reprise des activites apres sinistre"*

---

## Annexe : architecture technique en contexte cyber-resilience

```
Temps normal (mode advisory)                    Post-attaque (mode actif)
================================               ================================

  [Machine 1]    [Machine 2]                    [Machine 1']    [Machine 2']
  Agent (obs.)   Agent (obs.)                   Agent (actif)   Agent (actif)
     |               |                              |               |
     +----> Gateway <----+                          +----> Gateway <----+
                |                                              |
            Backend                                        Backend
           (PostgreSQL)                                   (PostgreSQL)
                |                                              |
          DAG + historique                               DAG + historique
          (le blueprint)                            (le meme blueprint intact)
                                                              |
                                                    Reconstruction sequencee
                                                    niveau par niveau, avec
                                                    verification automatique
```

**Le blueprint survit a l'attaque** car le backend et sa base PostgreSQL font partie des premiers elements restaures (niveau 0 de la reconstruction). Une fois AppControl restaure, il contient tout le savoir necessaire pour reconstruire le reste.

---

## Prochaines etapes

1. **Prise de contact** : presentation de 45 minutes au CTO et a l'equipe infrastructure de Linedata
2. **Qualification** : identification des 3-5 applications critiques pour le POC
3. **POC** : 3 semaines en mode advisory, 1 jour de simulation DR
4. **Decision** : sur la base de resultats mesurables, pas de promesses

---

*AppControl -- Parce que la question n'est pas "si" une cyberattaque arrivera, mais "combien de temps" il faudra pour reconstruire.*
