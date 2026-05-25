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
| 00:35–00:40 | Toast en bas à droite : *"Audit signé — chaîne SHA-256 vérifiée."* | Apparition douce |
| 00:40–00:45 | La timeline d'audit s'ouvre dans AppControl : quatre lignes (action, état pré, état post, hash chaîné au précédent). Survol d'une ligne → badge *"Signé · vérifié"*. | Slow zoom sur la chaîne de hash |

### Narration (voix off)

> *Dimanche, trois heures dix-sept. Le batch core banking a planté.*
> *Votre senior sysadmin est en vacances.*
> *Vous ouvrez AppControl. La branche en erreur est déjà identifiée.*
> *Un clic. Les composants redémarrent dans le bon ordre.*
> *Quatre minutes plus tard, tout est vert. L'audit est chaîné, signé, prêt à exporter.*

### Sous-titres (alternative silencieuse)

```
00:03 — Dimanche 3h17. Le batch core banking a planté.
00:08 — La branche en erreur est déjà identifiée.
00:14 — Un clic : Restart error branch.
00:22 — Redémarrage dans l'ordre du DAG.
00:38 — Audit chaîné, signé. Quatre minutes.
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
*« L'exercice DR est lancé. Phase 4. Un check d'intégrité échoue. »*

### Pourquoi ce scénario, pas la bascule "qui se passe bien"
Une bascule DR réussie du premier coup est la routine — vos scripts internes y arrivent à peu près. **Le vrai différenciateur d'AppControl, c'est le rollback propre quand quelque chose échoue en cours d'exercice.** C'est ce que personne d'autre ne sait faire, et c'est ce qui transforme une bascule annuelle en *exercice de continuité réellement réversible*.

### État initial (seed)
- Carte `core-banking` chargée, badge *"Site actif : Paris"*
- Site `lyon` configuré
- Bascule déjà initiée — la vidéo commence en cours de phase 1
- Phase 4 contient un check d'intégrité scripté pour échouer (lag de réplication > seuil)

### Storyboard

| Temps | Visuel | Action |
|---|---|---|
| 00:00–00:04 | Vue DAG, badge *"Site actif : Paris"*. Bandeau *"Bascule DR Paris → Lyon — en cours"*. | — |
| 00:04–00:14 | Phases 1, 2, 3 défilent en accéléré (label *"× 4 vitesse"*) — drainage, arrêt ordonné, réplication. Tout se passe bien. | Accéléré |
| 00:14–00:24 | **Phase 4 : Start in Lyon** — vitesse normale. Composants passent jaune → vert un par un. | Animation cascade |
| 00:24–00:30 | Un check d'intégrité échoue : *"reconciler — replica lag 14 s — seuil 2 s"*. Le composant repasse en rouge. Bandeau rouge en haut : *"Anomalie détectée — Phase 4 — exercice suspendu"*. | Apparition bandeau |
| 00:30–00:34 | Deux boutons s'affichent : **Rollback** / **Force-continue (avec dérogation)**. Clic **Rollback**. | Click |
| 00:34–00:40 | Modal de confirmation : *"Retour à Paris. Annulation propre des phases 4, 3, 2, 1. Durée estimée : 18 s. Service jamais interrompu."* Clic **Confirm**. | Click |
| 00:40–00:55 | Animation inversée : Lyon se vide proprement, réplication inversée, Paris reprend. Composants reviennent en vert sur Paris dans l'ordre du DAG. | Animation cascade inverse |
| 00:55–01:00 | Badge *"Site actif : Paris (rétabli)"*. Timeline d'audit visible : exercice annoté *"anomalie détectée phase 4 / rollback réussi / replica lag à corriger hors-bande"*. | Slow scroll |

### Narration

> *Exercice annuel de bascule. Paris vers Lyon.*
> *Phases 1, 2, 3 — propres.*
> *Phase 4. Un check d'intégrité échoue : retard de réplication de quatorze secondes.*
> *Vous cliquez : Rollback.*
> *Annulation propre. Dix-huit secondes plus tard, vous êtes revenus sur Paris.*
> *Service jamais interrompu. Anomalie nommée. Décision documentée.*

### Sous-titres

```
00:04 — Bascule DR Paris → Lyon, en cours.
00:14 — Phase 4 — Démarrage à Lyon.
00:24 — Anomalie : replica lag 14 s.
00:34 — Clic : Rollback.
00:40 — Annulation propre, ordre inverse.
00:55 — Retour à Paris. Service jamais interrompu.
```

### Sélecteurs requis
- `data-testid="dr-phase-progress"`
- `data-testid="dr-anomaly-banner"`
- `data-testid="dr-rollback-button"`
- `data-testid="dr-confirm-rollback"`
- `data-testid="dr-active-site-badge"`
- `data-testid="audit-timeline"`

### Pourquoi ce scénario marche
La bascule DR est le rite annuel le plus stressant de toute prod bancaire — pas parce qu'elle peut ne pas marcher, mais parce qu'**en cas de problème, on n'a souvent pas de plan B propre**. Montrer un rollback documenté en moins d'une minute transforme l'exercice DR d'épreuve à risque en répétition contrôlée. C'est exactement le genre de capacité que DORA exige et que personne ne fournit aujourd'hui.

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
*« Comité de direction, mardi 9 h 30. Le DG demande si la prod tient. »*

### Pourquoi cette mise en scène
Une démo MCP en mode "tech" (terminal côte à côte avec une UI) impressionne les ingénieurs et glisse sur les dirigeants. À l'inverse, **un comité de direction est le moment où une réponse instantanée est physiquement impossible avec les outils actuels** : il faut appeler la prod, attendre, agréger plusieurs dashboards. AppControl + MCP rendent une réponse exacte en deux secondes. C'est ce contraste qui déclenche le *« attendez, c'est dangereux ce truc »*.

### Format
**Un seul écran.** AppControl en plein cadre. Une bulle de chat en surimpression dans le coin inférieur droit, comme un assistant intégré. Pas de split-screen avec terminal — illisible en GIF README et étranger à la cible.

### État initial (seed)
- Vue d'accueil AppControl chargée — dashboard agrégé multi-applications
- Bulle de chat en surimpression connectée au serveur MCP du crate `mcp/`
- Tout est globalement vert, sauf un voyant orange sur l'application `reporting`

### Storyboard

| Temps | Visuel principal | Bulle Claude | Action |
|---|---|---|---|
| 00:00–00:05 | Dashboard agrégé : 12 applications, 11 vertes, 1 orange | Bulle vide en bas à droite | — |
| 00:05–00:12 | L'utilisateur tape dans la bulle : *« Tout va bien en prod ce matin ? »* | Question apparaît | Type |
| 00:12–00:25 | Sur le dashboard, l'application `reporting` clignote doucement. Bulle Claude : *« Oui, sauf reporting : composant kpi-extractor redémarré une fois cette nuit, à 04:12, sans impact utilisateur. Tout le reste : nominal. »* | Réponse Claude + highlight UI | Animation + texte |
| 00:25–00:30 | L'utilisateur tape : *« Détaille reporting. »* | Question apparaît | Type |
| 00:30–00:42 | Le panneau `reporting` s'ouvre en zoom. Timeline du composant kpi-extractor : crash 04:11:38, restart auto 04:12:02, retour à RUNNING 04:13:08. | *« Crash mémoire à 04:11. Redémarrage automatique réussi en 1 min 06. Aucun rapport client raté. »* | Animation panneau + Claude |

### Narration

> *Mardi, neuf heures et demie. Comité de direction.*
> *Le DG demande si la prod tient.*
> *Vous tapez une phrase. AppControl répond.*
> *Pas un dashboard. Pas un graphe. Une réponse exacte, en deux secondes.*
> *Aucun outil d'exploitation existant n'est capable de cela aujourd'hui.*

### Sous-titres

```
00:00 — Comité de direction. « La prod tient ? »
00:08 — Une question, en langage naturel.
00:14 — Réponse immédiate, fondée sur l'état réel.
00:32 — Détail à la demande, sans changer d'outil.
00:42 — Aucun outil d'exploitation ne le permet aujourd'hui.
```

### Sélecteurs requis
- `data-testid="dashboard-overview"`
- `data-testid="mcp-chat-bubble"`
- `data-testid="mcp-chat-input"`
- `data-testid="app-card-reporting"`
- `data-testid="component-timeline"`

### Outillage spécifique
La bulle de chat est rendue par le frontend AppControl lui-même (composant `<McpChatBubble />` connecté au serveur MCP via WebSocket). Tout est filmé en un seul plan Playwright. Pas d'asciinema, pas de FFmpeg `hstack`. Plus simple, plus stable, plus fidèle à la cible.

### Pourquoi ce scénario marche
Il transforme la fonctionnalité technique (serveur MCP) en **valeur exécutive** (réponse immédiate à un dirigeant). C'est le scénario qui justifie une enveloppe budgétaire stratégique, pas un achat de tableau de bord.

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
