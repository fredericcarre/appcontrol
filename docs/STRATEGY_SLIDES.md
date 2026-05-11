---
marp: true
theme: default
paginate: true
size: 16:9
header: 'AppControl — Stratégie & Vision'
footer: 'Document interne'
---

# AppControl
## Une plateforme transverse de maîtrise opérationnelle

**Captation + Réconciliation + Exploitation de la connaissance applicative**

Présentation Direction

---

## 1. Le constat — la connaissance applicative est éclatée

Aucune source ne décrit **comment l'application tourne réellement en production aujourd'hui** :

- **CMDB** — briques techniques, serveurs, middlewares
- **XL Release / XL Deploy** — pipelines, manifestes, mapping composant↔serveur, ordre de déploiement
- **Base d'incidents** — dépendances révélées par les pannes (vérité terrain)
- **Schémas d'architecture** — vue cible, souvent obsolète
- **Tickets de flux + référentiels de flux** — qui parle à qui, sur quels ports
- **Schedulers, runbooks, scripts, sachants** — connaissance tribale

> Le problème n'est pas la qualité de chaque référentiel — c'est leur **absence de réconciliation et d'exploitation opérationnelle**.

---

## 2. Stratégie en 2 phases — Phase 1 : CAPTATION

Reconstituer l'architecture **as-run** à partir de l'existant :

| Source | Donnée extraite |
|---|---|
| CMDB | Briques techniques, serveurs, middlewares |
| **XL Release** | Ordre de déploiement, dépendances pipeline, environnements |
| **XL Deploy** | Manifestes applicatifs, mapping composant↔serveur, packages |
| Référentiel de flux + tickets | Liens réseau, ports, protocoles |
| Base d'incidents | Dépendances cachées (co-occurrences de pannes) |
| Schémas d'architecture | Intention, regroupements logiques |
| Supervision / logs prod | Composants réellement actifs |

→ **Premier JSON de map**, réconcilié, reflétant ce qui tourne aujourd'hui.

**Force de XL** : donnée *fraîche* (regénérée à chaque déploiement) et *opérationnelle* (déjà utilisée en prod).

---

## 3. Stratégie en 2 phases — Phase 2 : EXPLOITATION

Le JSON devient la map AppControl de l'application :

- **Ajustement UI** par les équipes (dépendance graph en drag & drop)
- **Ou via Pull Request** — la map est versionnée comme du code (GitOps)
- Review par les sachants applicatifs et infra
- Évolution continue, audit complet

L'application devient **opérable** :

- Démarrage / arrêt séquencé respectant le DAG
- **Rebuild** orchestré (cas d'usage critique — slide suivante)
- Bascule DR en 6 phases
- Diagnostic 3 niveaux (santé / intégrité / infra)
- Audit DORA append-only

> On ne demande pas aux équipes de re-documenter. On part de l'existant et on leur soumet une carte qu'elles valident et corrigent.

---

## 4. Le cas d'usage qui change tout : le REBUILD

Reconstruire une application (cyber, ransomware, perte datacenter, corruption majeure) **exige** :

1. **Connaître** l'inventaire exact et à jour
2. **Restaurer** dans le bon ordre de dépendances
3. **Vérifier** que chaque composant repart sainement (health, intégrité, infra)
4. **Reconstituer** les flux et redémarrer les processus métier

| Sans plateforme transverse | Avec AppControl |
|---|---|
| Rebuild manuel, sous stress | Rebuild scripté par la map versionnée |
| Non reproductible | **Répétable en exercice** (DR drill) |
| Délai = jours/semaines | Délai mesuré, optimisé, prouvé |
| Dépendant des sachants disponibles | Conforme **DORA** (capacité de reprise auditée) |

> **Sans capacité de start / stop / check, il n'y a pas de rebuild possible. Ces capacités ne sont pas un risque — ce sont l'outil.**

---

## 5. Lever la peur des prods — garde-fous natifs

Un outil qui peut arrêter de la prod *doit* être encadré. Un outil qui ne peut rien faire ne reconstruit rien.

| Risque perçu | Réponse intégrée |
|---|---|
| « Quelqu'un va arrêter de la prod par erreur » | **RBAC** : `view < operate < edit < manage < owner` par application |
| « On n'aura pas confiance dans la map au début » | **Advisory mode** : agents en observation seule, zéro exécution |
| « On ne saura pas qui a fait quoi » | **Audit log append-only** (state_transitions, action_log) — DORA |
| « On veut simuler avant » | **Dry-run** : plan d'exécution calculé sans exécution |
| « Trafic en clair entre composants » | **mTLS partout** (agent ↔ gateway ↔ backend) |
| « Une action doit être validée » | **Mode PR-only** : start/stop nécessitent une PR mergée |

---

## 6. Adoption graduelle — à la main des prods

Les prods choisissent leur niveau, par application. AppControl ne **force** rien.

```
Niveau 0  Captation seule (lecture des référentiels)            → 0 risque
Niveau 1  Advisory : observation, pas d'exécution               → 0 risque
Niveau 2  Diagnostic actif (check_cmd) — pas de start/stop      → lecture process
Niveau 3  Opérations sous PR mergée (start/stop IaC-style)
Niveau 4  Opérations directes pour les rôles habilités
```

> Chaque application progresse à son rythme. La captation et l'advisory démarrent immédiatement, sans demander la confiance des prods.

---

## 7. Outil transverse vs. refonte des référentiels

L'alternative « refonder la CMDB et le référentiel de flux » est légitime mais **ne répond pas au même problème**.

| Axe | Refonte des référentiels | AppControl (transverse) |
|---|---|---|
| **Time-to-value** | 3 à 5 ans, fort risque d'échec | Semaines par application |
| **Nature de la donnée** | Reste **déclarative** (saisie humaine) | **Observée** (agents temps réel + réconciliation) |
| **Problème résolu** | Qualité par silo | **Réconciliation entre silos** + opération |
| **Coût** | Coût pur — un référentiel est un passif | Coût qui paie aussi rebuild, DR, audit |
| **Permet le rebuild ?** | Non — un référentiel ne reconstruit pas | Oui — c'est sa fonction |
| **Effet sur l'existant** | Remplace (politique) | **Enrichit en retour** la CMDB |
| **Réversibilité** | Faible | Forte (advisory → opérationnel progressif) |

---

## 8. Les deux démarches sont complémentaires

AppControl et une refonte des référentiels ne sont **pas concurrents** :

- AppControl **tire profit** d'une CMDB de qualité
- La CMDB **s'enrichit** de ce qu'AppControl observe (topologie réelle, dépendances révélées)
- Cercle vertueux : plus d'apps embarquées → meilleure donnée → meilleur outillage transverse

**Différence de timing** :
- Refonte référentiels = **bénéfice dans 3-5 ans** (si succès)
- AppControl = **bénéfice opérationnel dès le 1er jour** (captation), opérationnel complet en quelques mois

---

## 9. Bénéfices par audience

**Production / SRE**
- Démarrage / arrêt séquencés automatiquement (fin des scripts shell éparpillés)
- Restart ciblé sur branche en erreur (pink branch)
- Bascule DR orchestrée en 6 phases, testable et chronométrée
- Diagnostic 3 niveaux : santé process / intégrité données / infra OS

**Équipes applicatives**
- La map est **vivante** (agents temps réel) — fini les schémas Visio obsolètes
- Onboarding accéléré : la map *est* la documentation
- Intégration **sans rupture** avec Control-M, AutoSys, $U, TWS

**Gouvernance / Direction**
- Conformité **DORA** : audit trail complet et append-only
- MTTR réduit (bon ordre = pas de cascade d'incidents)
- DR mesurable et **prouvable**
- Réduction de la dette opérationnelle (un outil au lieu de N copies)

---

## 10. Pourquoi une solution *transverse*

1. **Effet réseau** — chaque application embarquée enrichit les dépendances vues depuis les autres. La 10ᵉ app coûte moins que la 1ʳᵉ.
2. **Donnée recyclée** — AppControl enrichit la CMDB en retour. On casse le cercle vicieux « CMDB jamais à jour ».
3. **Un seul point d'intégration** pour schedulers, releases, monitoring — au lieu d'intégrer chacun avec chaque application.
4. **Convergence des silos** — CMDB, supervision, schedulers, runbooks parlent enfin le même langage : la map.
5. **ROI croissant** — plus on capte, plus précise est la base ; plus elle est précise, plus rapide la captation suivante.

---

## 11. Message clé pour la direction

> **Le rebuild n'est pas une option, c'est une obligation DORA.**
> **Et le rebuild exige un outil qui sait *exécuter*, pas seulement *décrire*.**

AppControl :

- **Capte** la connaissance là où elle existe (CMDB, XLR/XLD, flux, incidents, schémas)
- **Réconcilie** ces sources en une carte vivante et versionnée
- **Rend opérable** cette carte (rebuild, DR, démarrage séquencé, audit DORA)

La peur des prods est levée par des garde-fous natifs (RBAC, advisory, dry-run, PR-mode, audit append-only) et une **adoption graduelle à la carte**.

Une refonte des référentiels et AppControl ne sont pas concurrents — AppControl rend la donnée existante **utile dès aujourd'hui**, et **améliore** les référentiels en retour.

---

## 12. Prochaines étapes proposées

1. **Sélection d'une application pilote** — idéalement avec un rebuild récent ou un DR planifié
2. **Phase 1 : captation** depuis CMDB, XLR/XLD, référentiel de flux — production d'un premier JSON de map
3. **Validation de la map** avec l'équipe applicative et la prod (mode advisory)
4. **Exercice de rebuild à blanc** sur environnement non-prod
5. **Bilan + plan d'embarquement** des applications critiques (top 20 DORA)

> Objectif : démontrer la valeur sur 1 application en 3 mois, embarquer 20 applications critiques en 12 mois.
