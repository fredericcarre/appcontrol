# Scénarios de démo — AppControl

Ce document est la **source de vérité narrative** des démos vidéo intégrées au README et au site. Chaque scénario est exploitable à plusieurs niveaux :

- texte court intégré au README (déjà fait)
- spec Playwright pour l'enregistrement automatique (à venir)
- texte de voix off / sous-titres regénérable à chaque release par Claude à partir du CHANGELOG (à venir)

## Principes

- **Une thèse par scénario.** Chaque démo doit illustrer un aspect de la thèse principale : *vous voyez l'application, pas l'infrastructure*.
- **Durée serrée.** 30 à 60 secondes. Au-delà, on perd 80 % du public en lecture passive sur le README.
- **Pas de narration sur la voix off du temps mort.** Ce qui se voit n'a pas besoin d'être dit. La voix complète, elle ne décrit pas.
- **Sous-titres présents par défaut**, voix off optionnelle. Beaucoup de lecteurs regardent en silence (réunion, métro).
- **Pas de mention IA dans la vidéo finale.** Le pipeline est notre affaire, pas celle du spectateur.

## Inventaire

| # | Slug | Durée | Placement README |
|---|---|---|---|
| 1 | `incident-recovery` | ~45 s | Section *Dimanche 3h17* |
| 2 | `dr-switchover` | ~60 s | Section *Mardi 14h* |
| 3 | `audit-export` | ~30 s | Section *Vendredi 10h* |
| 4 | `mcp-claude-control` | ~45 s | Section *Le quatrième moment* |
| 5 *(V2)* | `kube-blind-spot` | ~30 s | Section *L'angle mort de Kubernetes* |

---

## 1. `incident-recovery` — Dimanche 3h17

### Hook
*« Dimanche 3h17. Le batch core banking a planté. Votre senior sysadmin est en vacances. »*

### État initial (seed)
- Carte `core-banking` chargée depuis `examples/banking-core-system.json`
- Trois composants en état `FAILED` sur la branche batch : `batch-loader → reconciler → reporter`
- Le reste de l'application en `RUNNING`

### Storyboard

| Temps | Visuel | Action |
|---|---|---|
| 00:00–00:03 | Écran noir → notification d'astreinte qui apparaît : *"core-banking — branche batch en erreur"* | Fade in |
| 00:03–00:08 | Ouverture de la carte `core-banking`. Vue DAG complète. La branche batch est rouge, le reste vert. | Pan léger sur la branche rouge |
| 00:08–00:12 | Survol d'un nœud rouge. Tooltip : *"reconciler — exit code 1 — 03:14:22"* | Pause sur le tooltip |
| 00:12–00:16 | Clic sur le bouton **Restart error branch** | Click visible |
| 00:16–00:20 | Modal de confirmation : *"3 composants à redémarrer. Ordre : batch-loader → reconciler → reporter."* Clic **Confirm**. | Click |
| 00:20–00:35 | Animation : `batch-loader` passe en `STARTING` (jaune) puis `RUNNING` (vert). Puis `reconciler`, puis `reporter`. Ligne de temps en bas qui défile. | Animation cascade |
| 00:35–00:40 | Toast en bas à droite : *"Audit signed. Sent to ops@bank.fr."* | Apparition douce |
| 00:40–00:45 | Vue email : objet *"\[AppControl\] core-banking — branche batch restored 03:18:42"*. Aperçu du PDF d'audit. | Slow zoom sur le PDF |

### Narration (voix off)

> *Dimanche, trois heures dix-sept. Le batch core banking a planté.*
> *Votre senior sysadmin est en vacances.*
> *Vous ouvrez AppControl. La branche en erreur est déjà identifiée.*
> *Un clic. Les composants redémarrent dans le bon ordre.*
> *Quatre minutes plus tard, tout est vert. L'audit signé est dans votre boîte mail.*

### Sous-titres (alternative silencieuse)

```
00:03 — Dimanche 3h17. Le batch core banking a planté.
00:08 — La branche en erreur est déjà identifiée.
00:14 — Un clic : Restart error branch.
00:22 — Redémarrage dans l'ordre du DAG.
00:38 — Audit signé, envoyé. Quatre minutes.
```

### Sélecteurs requis (à ajouter au frontend)
- `data-testid="map-canvas"`
- `data-testid="restart-error-branch-button"`
- `data-testid="confirm-action-button"`
- `data-testid="audit-toast"`

### Pourquoi ce scénario marche
La douleur est universelle (tout DSI a vécu un incident week-end). La promesse est tangible (4 minutes vs 4 heures). La preuve est complète (action + audit + email).

---

## 2. `dr-switchover` — Mardi 14h

### Hook
*« Exercice annuel de bascule de site. Toute la direction de la prod regarde. »*

### État initial (seed)
- Carte `core-banking` chargée
- Tous les composants en `RUNNING` sur le site `paris`
- Site `lyon` configuré, vide
- Mode démo : durée des phases compressée pour tenir dans la vidéo

### Storyboard

| Temps | Visuel | Action |
|---|---|---|
| 00:00–00:04 | Vue DAG, badge *"Site actif : Paris"* en haut | — |
| 00:04–00:08 | Clic sur **DR Switchover**. Modal avec 6 phases listées : *Quiesce → Stop → Replicate → Start → Verify → Resume*. | Click |
| 00:08–00:12 | Choix : Target = Lyon, Mode = FULL. Clic **Initiate**. | Click |
| 00:12–00:20 | **Phase 1 : Quiesce traffic.** Composants passent en *DRAINING*. Trafic visualisé qui décroît. | Animation barre de progression |
| 00:20–00:28 | **Phase 2 : Stop applications.** Reverse-DAG : composants passent en gris dans l'ordre inverse. | Animation cascade inverse |
| 00:28–00:36 | **Phase 3 : Replicate state.** Visualisation de réplication Paris → Lyon (flèche animée). | Animation transversale |
| 00:36–00:44 | **Phase 4 : Start in Lyon.** Site Lyon devient actif. Composants passent jaune → vert dans l'ordre du DAG. | Animation cascade |
| 00:44–00:50 | **Phase 5 : Verify integrity.** Liste de checks qui se cochent en vert. | Tick animations |
| 00:50–00:55 | **Phase 6 : Resume traffic.** Badge *"Site actif : Lyon"* prend la place de Paris. | Transition |
| 00:55–01:00 | Aperçu du rapport de bascule : timestamps, signatures, checks. | Slow zoom |

### Narration

> *Mardi, quatorze heures. Exercice annuel de bascule de site.*
> *Six phases. Rollback possible à chaque étape.*
> *Drainer le trafic. Arrêter dans l'ordre inverse. Répliquer. Démarrer à Lyon. Vérifier. Reprendre.*
> *Vous voyez chaque composant changer de site, en temps réel.*
> *Le rapport de conformité est prêt avant la fin de la réunion.*

### Sous-titres

```
00:04 — Bascule DR : Paris → Lyon.
00:12 — Phase 1 — Drainage du trafic.
00:20 — Phase 2 — Arrêt ordonné, DAG inverse.
00:28 — Phase 3 — Réplication d'état.
00:36 — Phase 4 — Démarrage à Lyon.
00:44 — Phase 5 — Vérification d'intégrité.
00:50 — Phase 6 — Reprise du trafic.
00:55 — Rapport de bascule signé.
```

### Sélecteurs requis
- `data-testid="dr-switchover-button"`
- `data-testid="dr-target-site-select"`
- `data-testid="dr-mode-select"`
- `data-testid="dr-initiate-button"`
- `data-testid="dr-phase-progress"`
- `data-testid="dr-active-site-badge"`

### Pourquoi ce scénario marche
La bascule DR est le rite annuel le plus stressant de toute prod bancaire. La compresser à 60 secondes visibles est physiquement frappant. C'est là que l'auditeur se dit *« attendez, ça fait ça en une minute ? »*

---

## 3. `audit-export` — Vendredi 10h

### Hook
*« L'ACPR demande la trace de la dernière bascule. »*

### État initial (seed)
- Page **Reports** chargée
- Historique réel d'événements sur les 30 derniers jours, incluant la bascule du scénario 2

### Storyboard

| Temps | Visuel | Action |
|---|---|---|
| 00:00–00:03 | Boîte mail. Subject : *"ACPR — Demande de traçabilité — bascule DR mars 2026"*. | — |
| 00:03–00:07 | Pivot vers AppControl. Page Reports. Date picker ouvert : *"Last 30 days"*. | Click date picker |
| 00:07–00:11 | Filtre type d'événement : *Switchover events*. La liste se met à jour. 47 événements. | Click filter |
| 00:11–00:15 | Survol d'une ligne : timestamp, utilisateur, signature SHA-256, hash du précédent (chaînage). | Pause survol |
| 00:15–00:19 | Clic **Export DORA**. Modal : *"Generating signed report..."* avec spinner. | Click |
| 00:19–00:25 | Aperçu PDF qui défile : page de garde, timeline, action log, state transitions, signatures, hash chain. | Slow scroll |
| 00:25–00:30 | Téléchargement. Fichier `dora-export-2026-03.pdf` apparaît dans la barre de téléchargements. | — |

### Narration

> *Vendredi, dix heures. L'ACPR demande la trace de la dernière bascule.*
> *Vous filtrez la période. Vous filtrez le type d'événement.*
> *Vous cliquez : Export DORA.*
> *Tout est là : signé, daté, immuable. Qui a fait quoi, quand, et pourquoi.*
> *Le rapport part chez le régulateur dans la matinée.*

### Sous-titres

```
00:03 — Demande ACPR : tracer la bascule.
00:07 — Filtre période et type d'événement.
00:15 — Export DORA en un clic.
00:19 — Audit chaîné, signé, immuable.
00:25 — Rapport prêt à envoyer.
```

### Sélecteurs requis
- `data-testid="reports-date-picker"`
- `data-testid="reports-event-type-filter"`
- `data-testid="export-dora-button"`
- `data-testid="audit-pdf-preview"`

### Pourquoi ce scénario marche
DORA et l'ACPR sont des sujets de réveil pour tout DSI banque/assurance. Montrer en 30 secondes que ce qui prend habituellement deux jours de consultants est un clic — c'est le scénario le plus *rentable* commercialement.

---

## 4. `mcp-claude-control` — Le quatrième moment

### Hook
*« Pilotez votre prod en langage naturel. »*

### Format
**Split-screen.** Terminal Claude à gauche (50 %), UI AppControl à droite (50 %).

### État initial (seed)
- Claude CLI connecté au serveur MCP AppControl (binaire du crate `mcp/`)
- Carte `core-banking` chargée à droite, avec un composant `payment-gateway` en état `DEGRADED`

### Storyboard

| Temps | Visuel gauche (terminal) | Visuel droit (UI) | Action |
|---|---|---|---|
| 00:00–00:05 | Prompt vide. L'utilisateur tape : *"Quelles applications sont en état dégradé ce matin ?"* | Vue d'ensemble du dashboard | Type |
| 00:05–00:12 | Claude répond : *"Une application : core-banking. Composant payment-gateway, latence anormale depuis 03:42."* | Le composant `payment-gateway` clignote en orange | Réponse Claude + highlight UI |
| 00:12–00:20 | L'utilisateur tape : *"Diagnostique-le, niveau 2."* | — | Type |
| 00:20–00:30 | Claude exécute le tool `diagnose_app`. Réponse : *"Test d'intégrité : DB primary OK, replica lag 8s (seuil : 2s). Cause probable : saturation réseau site secondaire."* | Le panel diagnostic s'ouvre à droite, mêmes informations affichées graphiquement | Tool call animé |
| 00:30–00:38 | L'utilisateur tape : *"Redémarre payment-gateway en dry-run d'abord."* | — | Type |
| 00:38–00:45 | Claude répond avec le plan : ordre, durée estimée, impact. Demande confirmation. | Plan d'exécution surimprimé sur la carte | Plan animé |

### Narration

> *Quatrième moment. Vous parlez à votre production en langage naturel.*
> *Vous demandez ce qui ne va pas. Le diagnostic. Le plan d'action.*
> *Tout ce que vous tapez, vous le voyez aussi à l'écran.*
> *AppControl expose un serveur MCP natif. Aucun outil d'exploitation existant ne le permet aujourd'hui.*

### Sous-titres

```
00:00 — Pilotez votre prod en langage naturel.
00:08 — Claude voit ce que vous voyez.
00:20 — Diagnostic niveau 2, en deux phrases.
00:38 — Plan d'exécution en dry-run.
00:45 — Aucun outil d'exploitation existant ne le permet aujourd'hui.
```

### Outillage spécifique
- Capture terminal : `asciinema` puis conversion en vidéo via `agg`
- Composition split-screen : FFmpeg `hstack` filter
- Synchronisation : timestamps communs entre asciinema et Playwright

### Pourquoi ce scénario marche
C'est le scénario qui déclenche le *« attends, c'est dangereux ce truc »* le plus rapidement. C'est le seul du marché à ce niveau de fonctionnalité. Le split-screen rend la magie visible : Claude *voit* la même chose que l'opérateur.

---

## 5. *(V2)* `kube-blind-spot` — L'angle mort de Kubernetes

### Hook
*« Les pods tournent. L'application est cassée. »*

### Format
**Split-screen.** Dashboard Kube standard (Lens / OpenShift Console / k9s) à gauche, AppControl à droite.

### Storyboard (synthèse)

- 00:00–00:08 : À gauche, tous les pods sont `Running`. Tout vert. À droite, AppControl montre la carte applicative — une branche est rouge.
- 00:08–00:18 : Zoom sur le pod le plus visité (à gauche) : healthcheck OK, CPU OK, mémoire OK. Zoom sur le composant rouge à droite : *"reconciler — sortie de batch attendue à 03:00, pas reçue"*.
- 00:18–00:25 : Légende qui apparaît : *"Kubernetes voit l'infrastructure. AppControl voit l'application."*

### Narration

> *Vos pods tournent. Vos métriques sont vertes. Votre tableau de bord d'infrastructure dit : tout va bien.*
> *Mais votre batch quotidien n'a pas livré son fichier.*
> *Kubernetes ne vous le dira jamais. AppControl, oui.*

### Pourquoi le garder pour la V2
Plus complexe à seed (il faut un cluster Kube de démo crédible). Mais c'est la **preuve visuelle de la thèse principale du README**. À industrialiser une fois les 4 premiers scénarios validés.

---

## Prochaines étapes

1. **Validation par toi** de ces 4 (+1) scénarios — ajustements narratifs avant tout codage.
2. **Ajout des `data-testid`** listés ci-dessus dans le frontend (mini-PR séparée).
3. **Préparation des seeds** : `examples/demo-seeds/incident.json`, `examples/demo-seeds/dr-baseline.json`, etc.
4. **Écriture des specs Playwright** : `frontend/e2e-demos/01-incident-recovery.spec.ts` etc.
5. **Pipeline CI** : `.github/workflows/demo-videos.yaml` qui produit MP4 + GIF en assets de release et remplace les marqueurs `<!-- VIDEO:xxx -->` du README.
