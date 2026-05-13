---
title: "Répondre avec sérénité aux exigences de DORA"
subtitle: "AppControl — capter, réconcilier et exploiter la connaissance applicative pour industrialiser la résilience opérationnelle et prouver la conformité réglementaire."
author: "Invivoo"
date: "Mai 2026"
lang: fr-FR
toc: true
toc-depth: 2
documentclass: article
geometry:
  - margin=2.2cm
---

::: {custom-style="DraftBanner"}
**DRAFT — Version 0.9 — Ne pas diffuser — © 2026 Invivoo, tous droits réservés**
:::

**Document** Note de synthèse stratégique\
**Version** 0.9 — Draft\
**Classification** Confidentiel — Ne pas diffuser\
**Copyright** © 2026 Invivoo — Tous droits réservés\
**Usage** Argumentaire direction (interne ou externe)

---

# Synthèse exécutive

Dans toute entreprise mature, la connaissance applicative est éclatée entre CMDB, outils de déploiement (XL Release / XL Deploy), bases d'incidents, référentiels de flux, schémas d'architecture, runbooks et sachants. Aucune source ne décrit comment l'application **tourne réellement en production**. AppControl résout ce problème en deux temps : d'abord en **captant et réconciliant** ces données pour produire une carte vivante, puis en **exploitant** cette carte pour opérer (démarrage séquencé, rebuild, bascule DR, audit). Le caractère transverse n'est pas un détail d'architecture — c'est ce qui rend la résilience prouvable, mesurable et industrialisable, comme l'exige le règlement européen DORA en vigueur depuis janvier 2025.

---

# 1. Le constat — la connaissance applicative est éclatée

Dans la plupart des grandes entreprises, la connaissance nécessaire pour **opérer** une application est dispersée entre des dizaines de sources, chacune partielle et déclarative.

| Source | Donnée disponible | Limite |
|---|---|---|
| **CMDB** | Briques techniques, serveurs, middlewares | Souvent obsolète ou incomplète |
| **XL Release / XL Deploy** | Pipelines, manifestes, mapping composant↔serveur, ordre de déploiement | Vue déploiement, pas opération |
| **Base d'incidents** | Dépendances révélées par les pannes — vérité terrain | Jamais capitalisée |
| **Schémas d'architecture** | Vue cible | Figée dans Visio, déconnectée de la réalité |
| **Tickets & référentiel de flux** | Qui parle à qui, sur quels ports | Donnée précieuse, jamais exploitée |
| **Runbooks & sachants** | Schedulers, scripts, Confluence | Connaissance tribale, non capitalisée |

> Le problème n'est pas la qualité de chaque référentiel pris isolément. C'est leur **absence de réconciliation et d'exploitation opérationnelle**. Aucune de ces sources ne permet, seule, de redémarrer une application ou de la reconstruire.

---

# 2. La proposition de valeur — capter, réconcilier, exploiter

AppControl est conçu autour de trois actions complémentaires qui forment un cycle vertueux.

**1. Capter** — Récupérer la donnée là où elle existe déjà : CMDB, XLR, XLD, référentiel de flux, base d'incidents, schémas, supervision.

**2. Réconcilier** — Croiser ces sources, lever les contradictions, produire une carte vivante et versionnée — fidèle à la production.

**3. Exploiter** — Opérer à partir de cette carte : démarrage séquencé, rebuild, bascule DR, audit DORA, intégration scheduler.

Chaque application embarquée enrichit la base de connaissance commune, ce qui réduit le coût d'embarquement des suivantes et améliore la qualité des référentiels d'origine (effet retour vers la CMDB notamment).

---

# 3. L'approche en deux phases

## Phase 1 — Captation et réconciliation

| Source | Donnée extraite |
|---|---|
| CMDB | Briques techniques, serveurs, OS, middlewares |
| **XL Release** | Ordre de déploiement, dépendances pipeline, environnements |
| **XL Deploy** | Manifestes, mapping composant↔serveur, packages déployés |
| Référentiel de flux + tickets | Liens réseau entre composants, ports, protocoles |
| Base d'incidents | Dépendances cachées (co-occurrences de pannes) |
| Schémas d'architecture | Intention, regroupements logiques |
| Supervision / logs prod | Composants réellement actifs |

**Sortie** : un premier JSON de map par application, reflétant ce qui tourne aujourd'hui — pas ce qui est documenté quelque part.

La donnée XL est particulièrement précieuse : elle est *fraîche* (regénérée à chaque déploiement) et *opérationnelle* (déjà utilisée pour de vrai en production).

## Phase 2 — Exploitation

La map devient le point de référence opérationnel de l'application :

- Ajustement par les équipes via l'interface (graphe de dépendances en drag & drop)
- Ou via **Pull Request** — la map est versionnée comme du code (GitOps)
- Évolution continue, revue par les sachants applicatifs et infrastructure
- L'application devient opérable : start/stop séquencés, rebuild, bascule DR, audit append-only

> On ne demande pas aux équipes de re-documenter. On part de l'existant et on leur soumet une carte qu'elles n'ont plus qu'à **valider et corriger**.

---

# 4. De l'agrégation au plein potentiel — trajectoire de montée en puissance

La montée en puissance d'AppControl se fait par paliers. Chaque étape **ajoute de la donnée** et **débloque une capacité opérationnelle** supplémentaire. Aucune étape n'est obligatoire ni irréversible — chaque application progresse à son rythme.

## Phase A — Agrégation des données

### Étape 1 — Connexion aux référentiels

**Données captées**

- Accès lecture CMDB, XLR, XLD
- Référentiel de flux, base d'incidents
- Schémas d'architecture, supervision

**Capacités débloquées**

- Inventaire transverse consolidé
- Aucune action exécutée — risque nul

### Étape 2 — Réconciliation et génération de la map initiale

**Données captées**

- Composants déduits, dépendances inférées
- Contradictions inter-sources mises en évidence

**Capacités débloquées**

- Premier JSON de map automatique par application
- Vue *as-run* reconstituée

### Étape 3 — Validation par les équipes

**Données captées**

- Corrections, enrichissements via UI ou Pull Request
- Versionnement de la map (historique complet)

**Capacités débloquées**

- Carte de référence partagée et auditable
- Documentation vivante de l'application

## Phase B — Exploitation progressive

### Étape 4 — Observation passive (Advisory)

**Données captées**

- État réel des composants observé par les agents
- Dérives map déclarée vs. réalité production

**Capacités débloquées**

- Détection des écarts entre architecture cible et réelle
- Toujours zéro exécution — risque nul

### Étape 5 — Diagnostic actif (trois niveaux)

**Données captées**

- Santé process (niveau 1)
- Intégrité des données (niveau 2)
- État de l'infrastructure (niveau 3)

**Capacités débloquées**

- Détection précoce des incidents
- Recommandation automatique (Restart / Rebuild / Infrastructure)
- Toujours pas de start/stop — lecture seule

### Étape 6 — Opérations courantes

**Données captées**

- Audit log append-only des actions
- Métriques de durée start/stop par composant

**Capacités débloquées**

- Démarrage / arrêt séquencés respectant le DAG
- Restart ciblé sur branche en erreur (pink branch)
- Intégration scheduler (Control-M, AutoSys, $U, TWS)

### Étape 7 — Rebuild à blanc (drill)

**Données captées**

- RTR (Recovery Time for Rebuild) mesuré
- Comparaison entre drills successifs

**Capacités débloquées**

- Reconstruction complète sur environnement non-prod
- Chronométrage, répétabilité, conformité Article 11 DORA

### Étape 8 — Bascule DR opérationnelle

**Données captées**

- switchover_log de chaque bascule
- Mesure des temps par phase

**Capacités débloquées**

- 6 phases orchestrées avec rollback
- Bascule testable régulièrement, sans drame

### Étape 9 — Plein potentiel, DORA-ready

**Données captées**

- Cartographie ICT complète et à jour
- Registre append-only de toutes les actions
- RTO / RPO / RTR réels par application

**Capacités débloquées**

- Conformité DORA prouvable (Articles 8, 11, 12, 16, 25)
- Écosystème transverse : schedulers, releases, monitoring intègrent une seule source
- Effet retour : la CMDB est enrichie en continu

> La trajectoire est **cumulative** et **réversible**. Une application qui s'arrête à l'étape 5 (diagnostic seul) en tire déjà une valeur significative — détection précoce, cartographie à jour, conformité partielle. Les étapes 6 à 9 viennent **quand la confiance est établie**.

---

# 5. Le rebuild, justification stratégique de l'outil

Reconstruire une application — après cyber-incident, ransomware, perte de datacenter, corruption majeure — exige quatre capacités enchaînées :

1. **Connaître** l'inventaire exact et à jour
2. **Restaurer** les composants dans le bon ordre de dépendances
3. **Vérifier** que chaque composant repart sainement (santé, intégrité, infrastructure)
4. **Reconstituer** les flux et redémarrer les processus métier

## Comparatif

| Critère | Sans plateforme transverse | Avec AppControl |
|---|---|---|
| Mode opératoire | Manuel, sous stress | Scripté par la map versionnée |
| Reproductibilité | Aucune, jamais répété à blanc | **Répétable en exercice (drill)** |
| Délai | Jours / semaines | Mesuré, optimisé, prouvé |
| Dépendance | Sachants disponibles | Conforme DORA — audité |

::: {custom-style="CalloutDanger"}
**Sans plateforme transverse** — rebuild manuel et sous stress, non reproductible, jamais répété à blanc, délais en jours ou semaines, totalement dépendant des sachants disponibles.
:::

::: {custom-style="CalloutOK"}
**Avec AppControl** — rebuild scripté par la map versionnée, répétable en exercice (drill), délai mesuré et optimisé, conforme DORA avec capacité de reprise auditée.
:::

## Le rebuild à blanc — deux modes complémentaires

**1. Dry-run (simulation pure)** — un appel API avec `dry_run: true` retourne le plan complet (ordre DAG, commandes résolues, agents cibles) sans rien exécuter. Idéal pour revue et validation.

**2. Drill réel sur non-prod** — le même moteur exécute sur un site staging ou DR, chronométré, traçant le RTR dans l'audit log. C'est l'exercice qui prouve la conformité DORA.

::: {custom-style="PullQuoteWarn"}
Sans capacité de **start, stop et check**, il n'y a pas de rebuild possible. Ces capacités ne sont pas un risque — ce sont l'outil lui-même. Tout l'enjeu est de les rendre **sûres**, ce que les sections suivantes détaillent.
:::

---

# 6. DORA — pourquoi le rebuild est non négociable

**Digital Operational Resilience Act** — règlement européen 2022/2554, applicable depuis le **17 janvier 2025**. Périmètre : entités financières et leurs prestataires ICT (Information and Communication Technology) critiques. Sanctions : jusqu'à **2 % du chiffre d'affaires annuel mondial** pour l'entreprise, jusqu'à **1 M€** pour les dirigeants.

| Exigence DORA | Article | Réponse AppControl |
|---|---|---|
| Cartographier fonctions métier, actifs ICT et **interdépendances** | Art. 8 | Phase 1 — captation + réconciliation |
| Procédures de **reconstruction** après corruption ou cyberattaque | Art. 12 | Phase 2 — rebuild orchestré |
| Tests des plans de continuité, **au moins annuels** | Art. 11 | Rebuild à blanc + DR switchover répétables |
| Tests de **scénarios cyber** et reprise après corruption | Art. 25 | Drills chronométrés, comparables |
| **Registre** des incidents et actions de récupération | Art. 16 | Audit log append-only |
| **Prouver** le RTO / RPO réel | Art. 11-12 | RTR mesuré et tracé par exécution |

::: {custom-style="PullQuote"}
DORA n'exige pas seulement d'**avoir** un plan de reprise. Il exige de **prouver** qu'il fonctionne — testé, chronométré, tracé, auditable. Un PRA documenté sur Confluence ne suffit pas.
:::

Détail complet des cinq piliers et des articles clés en **annexe** (sections A1 à A10).

---

# 7. Lever la peur opérationnelle

La résistance des équipes de production face à un outil capable d'arrêter de la production est **légitime** et doit être adressée. AppControl le fait par conception, à deux niveaux : garde-fous techniques natifs et adoption graduelle pilotée par les prods.

## Garde-fous par conception

| Risque perçu | Réponse intégrée |
|---|---|
| « Quelqu'un va arrêter de la prod par erreur » | **RBAC granulaire** par application : `view < operate < edit < manage < owner` |
| « On n'aura pas confiance dans la map au début » | **Advisory mode** : agents en observation seule |
| « On ne saura pas qui a fait quoi » | **Audit log append-only** conforme DORA |
| « On veut simuler avant » | **Dry-run** : plan calculé sans exécution |
| « Trafic en clair entre composants » | **mTLS partout** |
| « Une action doit être validée » | **Mode PR-only** : start/stop nécessitent une PR mergée |

## Adoption graduelle — à la main des prods

Les cinq niveaux d'adoption correspondent aux étapes 1 à 6 de la trajectoire. Chaque application choisit son niveau, peut redescendre, et la captation/advisory démarrent **immédiatement** sans demander la confiance des prods. La confiance se construit ensuite, par paliers, sur la base de drills et d'expérience.

---

# 8. Outil transverse vs. refonte des référentiels

L'alternative classique — « refondons la CMDB et le référentiel de flux » — est légitime mais ne répond pas au même problème.

| Axe | Refonte des référentiels | AppControl (transverse) |
|---|---|---|
| **Time-to-value** | 3 à 5 ans, fort risque d'échec projet | Semaines par application |
| **Nature de la donnée** | Reste déclarative (saisie humaine) | **Observée** (agents temps réel + réconciliation) |
| **Problème résolu** | Qualité par silo | **Réconciliation entre silos** + opération |
| **Coût** | Pur — un référentiel est un passif | Paie aussi rebuild, DR, audit |
| **Permet le rebuild ?** | Non — un référentiel ne reconstruit pas | Oui — c'est sa fonction |
| **Effet sur l'existant** | Remplace (politique interne difficile) | **Enrichit en retour** la CMDB |
| **Réversibilité** | Faible (gros investissement) | Forte (advisory → opérationnel progressif) |

## Les deux démarches sont complémentaires

**Refonte des référentiels** : bénéfice dans 3 à 5 ans, si le projet aboutit. Politique interne lourde, périmètre flou, coût pur.

**AppControl** : bénéfice dès le premier jour (captation). Opérationnel complet en quelques mois. Améliore les référentiels en retour.

> Même une CMDB parfaite ne redémarre pas une application. Le vrai problème n'est pas la qualité de chaque référentiel — c'est leur absence de **réconciliation et d'exploitation opérationnelle**.

---

# 9. Pourquoi le caractère transverse change tout

1. **Effet réseau** — chaque application embarquée enrichit les dépendances vues depuis les autres. La 10ᵉ app coûte moins que la 1ʳᵉ. La 100ᵉ devient triviale.

2. **Donnée recyclée** — la topologie observée par AppControl alimente en retour la CMDB et le référentiel de flux. On casse le cercle vicieux « référentiels jamais à jour ».

3. **Un seul point d'intégration** pour schedulers, outils de release, supervision — au lieu d'intégrer chacun avec chaque application.

4. **Convergence des silos** — CMDB, supervision, schedulers, runbooks parlent enfin le même langage : la map.

5. **ROI croissant** — plus on capte, plus précise est la base ; plus elle est précise, plus rapide la captation suivante. Un outil par application n'aurait jamais cet effet.

---

# 10. Bénéfices par audience

## Production / SRE

- Start / stop séquencés automatiquement
- Restart ciblé sur branche en erreur
- Bascule DR orchestrée et testable
- Diagnostic 3 niveaux
- Fin des scripts shell éparpillés

## Équipes applicatives

- Map vivante (agents temps réel)
- Onboarding accéléré : la map est la documentation
- Intégration Control-M, AutoSys, $U, TWS sans rupture
- Évolution par Pull Request — IaC-friendly

## Direction / Gouvernance

- Conformité DORA — audit append-only
- MTTR réduit (plus de cascade)
- DR mesurable et prouvable
- Dette opérationnelle réduite
- Risque cyber et conformité maîtrisé

---

# 11. Message clé

::: {custom-style="PullQuote"}
**Le rebuild n'est pas une option, c'est une obligation DORA.** Et le rebuild exige un outil qui sait *exécuter*, pas seulement *décrire*.
:::

AppControl capte la connaissance là où elle existe (CMDB, XLR, XLD, flux, incidents, schémas), la réconcilie en une carte vivante et versionnée, puis la rend opérable — rebuild, DR, démarrage séquencé, audit. Le caractère transverse n'est pas un détail : c'est ce qui permet l'effet réseau, le recyclage des données vers les référentiels d'origine, et la mutualisation des intégrations.

La peur des équipes de production est levée par des garde-fous natifs (RBAC, advisory, dry-run, PR-mode, audit append-only) et une adoption graduelle pilotée par elles, application par application.

Une refonte des référentiels et AppControl ne sont pas concurrents — la première produit ses effets dans 3 à 5 ans *si* le projet aboutit, AppControl produit de la valeur dès le premier jour et améliore les référentiels en retour.

::: {custom-style="PullQuoteWarn"}
L'absence de capacité de rebuild prouvable n'est pas un confort manquant — c'est un **risque financier et personnel direct** dans le cadre DORA.
:::

---

# Annexe — DORA en détail

# A1. Qu'est-ce que DORA ?

**Digital Operational Resilience Act** — règlement européen **2022/2554**, applicable depuis le **17 janvier 2025**.

**Cible** : toutes les entités financières (banques, assurances, gestionnaires d'actifs, infrastructures de marché) et leurs prestataires ICT (Information and Communication Technology — TIC en français) critiques.

**Objectif** : garantir la résilience opérationnelle numérique — capacité à *fonctionner*, *résister* et *récupérer* face à des incidents ICT majeurs, y compris cyber.

DORA harmonise au niveau européen ce que chaque régulateur national imposait de façon hétérogène (ACPR en France, BaFin en Allemagne, etc.).

---

# A2. Les cinq piliers de DORA

| Pilier | Articles | Sujet |
|---|---|---|
| **Gestion du risque ICT** | 5–16 | Gouvernance, identification, protection, détection, réponse, récupération |
| **Reporting d'incidents** | 17–23 | Classification, déclaration des incidents majeurs aux autorités |
| **Tests de résilience** | 24–27 | Tests obligatoires des plans de continuité ; TLPT (Threat-Led Penetration Testing) tous les 3 ans pour entités significatives |
| **Risque tiers ICT** | 28–44 | Registre des contrats, supervision des prestataires critiques |
| **Partage d'information** | 45 | Échange volontaire sur les menaces cyber |

---

# A3. Article 8 — Identification

> *Les entités financières identifient et cartographient toutes les fonctions métier supportées par des ICT, les actifs ICT supportant ces fonctions, et leurs interdépendances, à un niveau de granularité approprié.*

## Implication concrète

- Inventaire à jour des composants ICT
- Mapping fonction métier ↔ composants techniques
- Dépendances entre composants (qui appelle qui, qui a besoin de qui)
- Granularité suffisante pour piloter une reprise

C'est exactement la **Phase 1 d'AppControl** : captation + réconciliation CMDB / XLR / XLD / référentiels de flux / incidents / schémas.

---

# A4. Article 11 — Politique de continuité d'activité ICT

> *Doit inclure des stratégies de réponse et de récupération, testées au moins annuellement, couvrant tous les scénarios de disruption significative.*

## Implication concrète

- Un plan écrit ne suffit pas — il doit être **testé**
- Fréquence minimale : **annuelle**
- Couverture : **tous les scénarios** de disruption significative

AppControl : drill de rebuild + DR switchover répétables sur environnement non-prod, chronométrés.

---

# A5. Article 12 — Sauvegarde, restauration, récupération

> *Doivent permettre de récupérer les systèmes ICT avec un impact minimal, et inclure des procédures de reconstruction des systèmes après corruption ou cyberattaque.*

## Implication concrète

- Le mot **« reconstruction »** est explicite dans le texte
- Capacité à reconstruire **après corruption majeure** (ransomware, attaque destructive)
- Impact minimal = procédures automatisables et mesurables

AppControl : moteur de rebuild — DAG order, bastion agent pour l'infra, site overrides, protection des composants critiques, suivi de complétion, vérification post-rebuild, SUSPEND sur échec.

---

# A6. Article 16 — Apprentissage et évolution

> *Les entités tiennent des registres des incidents ICT, dont les actions menées pour la récupération.*

## Implication concrète

- Trace de chaque action de récupération
- Qui, quoi, quand, sur quel composant, avec quel résultat
- Conservation auditable

## AppControl — audit log append-only

| Table | Contenu |
|---|---|
| `action_log` | Toutes les actions utilisateur, loguées avant exécution |
| `state_transitions` | Chaque changement d'état de composant |
| `switchover_log` | Chaque bascule DR |
| `check_events` | Chaque diagnostic |

Ces tables sont **append-only par règle critique** du projet — aucun UPDATE, aucun DELETE, jamais.

---

# A7. Article 25 — Tests des outils et systèmes ICT

> *Programmes de tests incluant des scénarios de cyber-incidents et de reprise après corruption majeure.*

## Implication concrète

- Tests **réalistes** (pas seulement théoriques)
- Scénarios **cyber** explicitement requis
- Reprise après corruption **majeure**

AppControl : drill de rebuild complet sur site non-prod, mesure du RTR, comparaison entre drills successifs pour identifier régressions ou améliorations.

---

# A8. Synthèse — exigence DORA vs. mécanisme AppControl

| Exigence DORA | Art. | Mécanisme technique |
|---|---|---|
| Cartographier interdépendances ICT | 8 | Map JSON versionnée, Phase 1 |
| Procédures de reconstruction | 12 | Moteur de rebuild (DAG order, bastion, protection) |
| Tests annuels des plans | 11 | Dry-run + drill staging |
| Scénarios cyber et corruption | 25 | DAG order + protection + bastion |
| Registre des actions de récupération | 16 | Audit log append-only (4 tables) |
| Mesure du RTO / RPO réel | 11-12 | RTR mesuré dans `action_log` |
| Granularité suffisante | 8 | DAG + FSM par composant |
| Protection composants critiques | 12 | Flag `rebuild_protected` |
| Suivi de complétion | 12, 25 | Polling + check + integrity_check |

---

# A9. Ce que DORA ne dit PAS

::: {custom-style="CalloutDanger"}
**DORA ne dit pas** : « Vous devez utiliser tel outil » · « Vous devez avoir telle architecture » · « Le RTO doit être de X heures ».
:::

::: {custom-style="CalloutOK"}
**DORA dit** : « Vous devez **prouver** que vous savez reconstruire » · « Vous devez **tester régulièrement** » · « Vous devez **documenter et tracer** chaque action » · « Vous devez **mesurer** votre temps de reprise réel ».
:::

::: {custom-style="PullQuoteWarn"}
Sans outil qui exécute la reconstruction, on ne peut ni la **tester** régulièrement, ni la **chronométrer**, ni la **prouver**. Le rebuild reste théorique — et donc non conforme.
:::

---

# A10. Sanctions et calendrier

| Item | Détail |
|---|---|
| **Application** | 17 janvier 2025 — déjà en vigueur |
| **Supervision** | ACPR en France ; EBA / EIOPA / ESMA au niveau européen |
| **Amendes entité** | Jusqu'à **2 % du chiffre d'affaires annuel mondial** |
| **Sanctions personnelles** | Jusqu'à **1 M€** pour les dirigeants |
| **Audit** | À tout moment : preuve des tests, registres d'incidents, cartographie ICT |

::: {custom-style="CalloutDanger"}
L'enjeu n'est pas seulement technique. L'**absence de capacité de rebuild prouvable** est un risque financier et personnel direct pour l'entreprise et ses dirigeants.
:::

---

::: {custom-style="DraftBanner"}
**Fin du document — DRAFT v0.9 — Ne pas diffuser — © 2026 Invivoo**
:::
