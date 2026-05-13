# Note de synthèse stratégique — formats de diffusion

Trois formats coexistent pour un même contenu, chacun pour un usage précis :

| Format | Fichier | Usage | Génération |
|---|---|---|---|
| **HTML** | `strategy.html` | Source de référence, lecture web | Édition directe |
| **PDF** | (généré) | Présentation, diffusion finale | Chrome → Imprimer → PDF |
| **DOCX** | (généré) | Relecture, annotations, *Track Changes* | `./build-docx.sh` |

## Génération PDF (présentation)

1. Ouvrir `strategy.html` dans **Chrome ou Edge** (Chromium — moteur le mieux supporté)
2. `Ctrl/⌘ + P`
3. Dans le dialogue : **Plus de paramètres** → décocher **« En-têtes et pieds de page »** (sinon le navigateur ajoute l'URL et la date)
4. Format : A4, marges par défaut
5. Cocher **« Graphiques d'arrière-plan »** pour préserver les couleurs
6. **Enregistrer en PDF**

Résultat : couverture sombre dégradée, watermark *DRAFT* diagonal, header/footer Invivoo répétés sur chaque page, chapitres sur pages séparées.

## Génération DOCX (relecture)

```bash
cd docs
./build-docx.sh
```

Prérequis : [Pandoc](https://pandoc.org/installing.html) installé.

```bash
brew install pandoc       # macOS
sudo apt install pandoc   # Ubuntu/Debian
choco install pandoc      # Windows
```

Le DOCX généré utilise des styles Word natifs (Titre 1, Titre 2, etc.), ce qui permet :
- Volet de navigation (`Affichage → Volet de navigation`)
- Track Changes et commentaires
- Modifications directes du contenu
- Export PDF depuis Word si besoin

### Gabarit Invivoo intégré

Le fichier `docs/invivoo-reference.docx` (versionné dans le repo) sert de gabarit Pandoc et apporte au DOCX généré :

- **Titres colorés** (Heading 1 sarcelle profond, Heading 2 sarcelle, Heading 3 sombre)
- **Saut de page automatique avant chaque Heading 1** — chaque chapitre démarre sur une nouvelle page
- **Pied de page sur chaque page** : `© 2026 Invivoo — Tous droits réservés · DRAFT v0.9 · numéro de page / total`
- **En-tête sur chaque page** : `INVIVOO · Confidentiel · Répondre avec sérénité aux exigences de DORA`
- **Styles personnalisés** invoqués par le Markdown via `::: {custom-style="..."}`  :
    - `DraftBanner` — bandeau Draft rouge centré (haut et bas du doc)
    - `PullQuote` — citation sarcelle avec bordure
    - `PullQuoteWarn` — citation ambre pour les avertissements
    - `CalloutOK` / `CalloutWarn` / `CalloutDanger` / `CalloutIndigo` — cartes de comparaison colorées
    - `Eyebrow` — kicker (petit texte en capitales sarcelle)
    - `TimelinePhaseA` / `TimelinePhaseB` — bannières de phases

### Régénérer ou modifier le gabarit

Le gabarit est produit par un script Python pour rester reproductible :

```bash
cd docs
pip install python-docx  # une fois
python3 build-reference-docx.py
```

Ouvre la palette de couleurs / polices / styles en haut de `build-reference-docx.py` pour ajuster la charte sans avoir à éditer le Word à la main.

Alternative manuelle : ouvrir `invivoo-reference.docx` dans Word, modifier les styles (Titre 1, Titre 2, DraftBanner, PullQuote, etc.), enregistrer — le prochain `./build-docx.sh` utilisera les modifications.

## Maintenir les deux sources synchronisés

Le HTML et le Markdown sont deux sources distinctes du même contenu. Quand une modification de contenu est apportée :

- **Modification mineure de wording** : appliquer dans les deux fichiers
- **Refonte structurelle** : modifier d'abord le Markdown (édition plus simple), puis répercuter dans le HTML

À terme, une CI GitHub Actions peut générer automatiquement le PDF et le DOCX à chaque push sur la branche.
