<!--
Release-only tail appended to README.md (FR) by
.github/workflows/release.yaml when mirroring the dev repo to
xcomponent/appcontrol-release. Replaces the dev-only "Démarrer" /
"Sous le capot" / "License" sections with their release equivalents
(binary install, no dev workflow, XComponent license).
The narrative above the RELEASE-CUT marker in README.md is shared
with the dev repo and is not duplicated here.
-->

## Installation

### Option 1 — Docker Compose (Linux / macOS)

```bash
gh release download --repo xcomponent/appcontrol-release --pattern 'docker-compose.release.yaml'
APPCONTROL_VERSION=latest docker compose -f docker-compose.release.yaml up -d
open http://localhost:8080
```

Connexion : `admin@localhost` / `admin`.

### Option 2 — Standalone PowerShell (Windows / Linux, sans Docker)

Aucune base à installer, aucun Docker requis. Un seul script gère tout.

```powershell
mkdir AppControl; cd AppControl
Invoke-WebRequest -Uri "https://github.com/xcomponent/appcontrol-release/raw/main/appcontrol.ps1" -OutFile appcontrol.ps1

.\appcontrol.ps1 install           # télécharge binaires + frontend
.\appcontrol.ps1 start             # démarre le backend
.\appcontrol.ps1 add-site Production
.\appcontrol.ps1 add-site DR-Site
```

Autres commandes : `stop`, `status`, `upgrade`, `logs [file]`, `help`. Compatible Windows PowerShell 5.1+ et PowerShell Core 6+.

### Option 3 — CLI uniquement (`appctl`)

```bash
gh release download --repo xcomponent/appcontrol-release --pattern 'appctl-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl-linux-amd64 && mv /usr/local/bin/appctl-linux-amd64 /usr/local/bin/appctl

export APPCONTROL_URL=http://localhost:3000
appctl login --email admin@localhost --password admin
appctl start core-banking --wait --timeout 120
appctl diagnose core-banking --level 2
appctl switchover core-banking --target-site lyon --mode FULL --wait
```

Variantes : `appctl-darwin-arm64`, `appctl-windows-amd64.exe`. Toutes disponibles dans les release assets.

### Images Docker

Toutes les images sont publiées sur GitHub Container Registry :

```bash
docker pull ghcr.io/xcomponent/appcontrol-backend:latest
docker pull ghcr.io/xcomponent/appcontrol-frontend:latest
docker pull ghcr.io/xcomponent/appcontrol-gateway:latest
docker pull ghcr.io/xcomponent/appcontrol-agent:latest
docker pull ghcr.io/xcomponent/appcontrol-init-certs:latest
```

### Helm sur OpenShift

```bash
gh release download --repo xcomponent/appcontrol-release --pattern 'appcontrol-*.tgz'
helm install appcontrol appcontrol-*.tgz --namespace appcontrol --create-namespace
```

---

## Cartes d'application prêtes à l'emploi

Trois exemples dans `appcontrol-docs-scripts.zip` :

| Exemple | Composants | Points clés |
|---|:---:|---|
| Three-Tier Web App | 7 | Dépendances fortes/faibles, réplication BDD, batch |
| Microservices E-Commerce | 12 | API gateway, message broker, service-per-DB |
| Core Banking System | 9 | Bascule DR Paris → Lyon, intégration Control-M, conformité DORA |

---

## Documentation

Documentation complète dans `appcontrol-docs-scripts.zip` :

- **[QUICKSTART.md](docs/QUICKSTART.md)** — Démarrage rapide
- **[USER_GUIDE.md](docs/USER_GUIDE.md)** — Guide utilisateur complet avec captures
- **[WINDOWS_DEPLOYMENT.md](docs/WINDOWS_DEPLOYMENT.md)** — Déploiement Windows
- **[AGENT_INSTALLATION.md](docs/AGENT_INSTALLATION.md)** — Installation des agents (toutes plateformes)
- **[CONFIGURATION.md](docs/CONFIGURATION.md)** — Toutes les options de configuration
- **[AZURE_GATEWAY.md](docs/AZURE_GATEWAY.md)** — Déploiement gateway sur Azure
- **[PRODUCTION_DEPLOYMENT.md](docs/PRODUCTION_DEPLOYMENT.md)** — Durcissement production

---

## Contact

Décrivez votre cas en trois lignes : un nom d'application, le scheduler en place, l'horizon DR à couvrir. Réponse sous 48h.

Prendre 15 minutes en visio pour une démonstration adaptée à votre stack.

`support@xcomponent.com`

---

## License

Copyright (c) 2024-2026 XComponent SAS. Tous droits réservés.

Ce logiciel est fourni sous forme de binaires pré-compilés pour évaluation et usage en production. La redistribution, le reverse engineering, et la mise à disposition sous forme de service managé sont interdits sans licence commerciale.

Contact : `support@xcomponent.com` pour les demandes de licence.
