# Plan: Discovery — De l'inventaire serveur à la map opérationnelle

## Vision

L'ingénieur de production reçoit une liste de serveurs (du CMDB/référentiel). Il déploie les agents AppControl. L'objectif : obtenir automatiquement une map exploitable avec les vrais composants, leurs dépendances, leurs commandes check/start/stop, et les infos de configuration. La map doit être immédiatement opérationnelle.

## Ce que l'agent doit collecter (par serveur)

| Donnée | Linux | Windows | Utilité |
|--------|-------|---------|---------|
| Processes + PID | sysinfo | sysinfo | Identifier les composants |
| TCP listeners (port→PID) | /proc/net/tcp + inode | netstat -ano | Typer les services (5432=PG, 5672=RabbitMQ...) |
| TCP connections (PID→remote) | /proc/net/tcp + inode | netstat -ano | Détecter dépendances inter-composants |
| Services système (→PID) | systemctl | sc query | Générer commandes start/stop/check |
| **Répertoire de travail** | /proc/[pid]/cwd | - | Trouver configs relatives |
| **Fichiers ouverts** | /proc/[pid]/fd | - | Détecter fichiers de config + logs |
| **Fichiers de config** | Parser cmdline + scan fd | - | Extraire connection strings |
| **Contenu config (extraits)** | Parser YAML/properties/.env/XML | - | Confirmer dépendances (JDBC URL, Redis host...) |
| **Fichiers de logs** | fd ouverts en écriture | - | Commandes custom "voir les logs" |
| **Cron jobs** | /var/spool/cron + /etc/cron.d | schtasks /query | Détecter les batchs |
| **Timers systemd** | systemctl list-timers | - | Détecter les batchs |
| Env vars (filtrées) | /proc/[pid]/environ | - | Confirmer connexions (DB_HOST, etc.) |

## Architecture du scan enrichi

### A. Nouveaux types dans `crates/common/src/types.rs`

```rust
/// A config file found open by a discovered process.
pub struct DiscoveredConfigFile {
    pub path: String,
    pub pid: u32,
    pub process_name: String,
    /// Extracted connection-relevant entries (host:port, URLs, DSNs)
    pub extracted_endpoints: Vec<ExtractedEndpoint>,
}

/// A connection endpoint extracted from a config file.
pub struct ExtractedEndpoint {
    pub key: String,            // "spring.datasource.url", "REDIS_HOST", etc.
    pub value: String,          // "jdbc:postgresql://db-srv:5432/orders"
    pub parsed_host: Option<String>,  // "db-srv"
    pub parsed_port: Option<u16>,     // 5432
    pub technology: Option<String>,   // "postgresql", "redis", "rabbitmq"
}

/// A log file found open by a discovered process.
pub struct DiscoveredLogFile {
    pub path: String,
    pub pid: u32,
    pub process_name: String,
    pub size_bytes: u64,
}

/// A scheduled job (cron, systemd timer, Windows Task Scheduler).
pub struct DiscoveredScheduledJob {
    pub name: String,
    pub schedule: String,       // "0 2 * * *" or "daily at 02:00"
    pub command: String,
    pub user: String,
    pub source: String,         // "crontab", "cron.d", "systemd-timer", "task-scheduler"
    pub last_run: Option<String>,
    pub enabled: bool,
}

/// Suggested operational commands for a discovered process.
pub struct CommandSuggestion {
    pub check_cmd: String,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub restart_cmd: Option<String>,
    pub confidence: String,     // "high" (systemd/service), "medium" (pidfile), "low" (pgrep)
    pub source: String,         // "systemd", "windows-service", "docker", "process"
}
```

Ajouter à `DiscoveredProcess` :
```rust
pub working_dir: Option<String>,
pub config_files: Vec<DiscoveredConfigFile>,
pub log_files: Vec<DiscoveredLogFile>,
pub command_suggestion: Option<CommandSuggestion>,
pub matched_service: Option<String>,  // nom du service systemd/Windows si trouvé
```

### B. Extension du protocole `AgentMessage::DiscoveryReport`

Ajouter le champ `scheduled_jobs: Vec<DiscoveredScheduledJob>` au variant `DiscoveryReport` dans `protocol.rs`.

### C. Agent Linux : scan approfondi (`discovery/linux.rs`)

#### C1. Répertoire de travail + fichiers ouverts

```
Pour chaque PID dans sys.processes():
  1. readlink(/proc/[pid]/cwd) → working_dir
  2. Lister /proc/[pid]/fd → pour chaque fd:
     - readlink → path
     - Si path commence par / (pas socket:, pipe:, anon_inode:):
       - stat le fichier → taille
       - Classifier:
         - Extension .log, .out, ou path contient /log/ → log_file
         - Extension .yml, .yaml, .properties, .conf, .cfg, .ini, .env, .xml, .json
           ET path PAS dans /proc, /sys, /dev → config_file
```

#### C2. Parsing des configs pour extraction d'endpoints

Pour chaque config_file trouvé, parser le contenu (max 64KB) selon le format :

| Format | Détection | Patterns recherchés |
|--------|-----------|---------------------|
| .properties / .env | `KEY=VALUE` | `*_HOST`, `*_PORT`, `*_URL`, `*_URI`, `*_DSN`, `jdbc:*`, `amqp://`, `redis://`, `mongodb://` |
| .yml / .yaml | YAML | Mêmes patterns, naviguer les clés imbriquées (spring.datasource.url, etc.) |
| .xml | Balises | `<url>`, `<connection-url>`, `<host>`, `<port>`, attributs `url=`, `host=` |
| .json | JSON | Mêmes patterns dans les valeurs string |

Extraction d'endpoint : parser les URLs/DSN pour extraire host + port + technologie :
- `jdbc:postgresql://host:5432/db` → host=host, port=5432, tech=postgresql
- `amqp://user:pass@rabbit-host:5672/vhost` → host=rabbit-host, port=5672, tech=rabbitmq
- `redis://redis-host:6379` → host=redis-host, port=6379, tech=redis
- `mongodb://mongo-host:27017/db` → host=mongo-host, port=27017, tech=mongodb
- `http://api-host:8080/endpoint` → host=api-host, port=8080, tech=http

#### C3. Cross-référence process ↔ service systemd

```
1. systemctl list-units --type=service → liste des services actifs
2. Pour chaque service actif:
   - systemctl show {service} --property=MainPID → PID
   - Si PID matche un process découvert → lier
   - Générer CommandSuggestion:
     check_cmd:   "systemctl is-active {service}"
     start_cmd:   "systemctl start {service}"
     stop_cmd:    "systemctl stop {service}"
     restart_cmd: "systemctl restart {service}"
     confidence:  "high"
     source:      "systemd"
3. Pour les processes sans service:
   check_cmd:  "pgrep -f '{cmdline_pattern}'"
   confidence: "low"
   source:     "process"
```

#### C4. Cron jobs + systemd timers

```
Cron:
  - Lire /var/spool/cron/crontabs/* (si permission)
  - Lire /etc/cron.d/*
  - Lire /etc/crontab
  - Filtrer : exclure logrotate, apt, certbot, etc.
  - Parser chaque ligne cron : schedule + command + user

Systemd timers:
  - systemctl list-timers --no-pager --no-legend
  - Pour chaque timer : schedule, unit associé, last trigger
```

### D. Agent Windows : scan approfondi (`discovery/windows.rs`)

#### D1. Cross-référence process ↔ Windows service

```
1. sc query type= service → nom + état + PID
2. Pour chaque service running avec PID matché:
   check_cmd:  "sc query {service} | findstr RUNNING"
   start_cmd:  "net start {service}"
   stop_cmd:   "net stop {service}"
   confidence: "high"
   source:     "windows-service"
3. Pour les autres:
   check_cmd:  "tasklist /FI \"IMAGENAME eq {name}\" | findstr /I {name}"
   confidence: "low"
```

#### D2. Scheduled Tasks

```
schtasks /query /fo CSV /v
  - Parser CSV
  - Filtrer : exclure tâches Microsoft, \Microsoft\*
  - Extraire : nom, schedule, command, user, état, dernière exécution
```

### E. Backend : corrélation enrichie (`api/discovery.rs`)

#### E1. Enrichir `POST /correlate`

La réponse inclut maintenant pour chaque service :
```json
{
  "services": [{
    "agent_id": "...",
    "hostname": "srv-02",
    "process_name": "java",
    "ports": [8080],
    "component_type": "service",
    "suggested_name": "order-api@srv-02",
    "command_suggestion": {
      "check_cmd": "systemctl is-active order-api",
      "start_cmd": "systemctl start order-api",
      "stop_cmd": "systemctl stop order-api",
      "confidence": "high",
      "source": "systemd"
    },
    "config_files": [
      { "path": "/opt/order-api/config/application.yml",
        "extracted_endpoints": [
          { "key": "spring.datasource.url", "value": "jdbc:postgresql://db-srv:5432/orders",
            "parsed_host": "db-srv", "parsed_port": 5432, "technology": "postgresql" },
          { "key": "spring.rabbitmq.host", "value": "rabbit-srv",
            "parsed_host": "rabbit-srv", "parsed_port": 5672, "technology": "rabbitmq" }
        ]
      }
    ],
    "log_files": ["/var/log/order-api/app.log"],
    "matched_service": "order-api.service"
  }],
  "dependencies": [
    // Deps TCP (déjà existant)
    { "from": 0, "to": 2, "inferred_via": "tcp_connection",
      "remote_addr": "10.0.0.3", "remote_port": 5432 },
    // NOUVEAU : deps confirmées par config
    { "from": 0, "to": 2, "inferred_via": "config_file",
      "config_key": "spring.datasource.url", "technology": "postgresql" }
  ],
  "scheduled_jobs": [
    { "hostname": "srv-02", "name": "nightly-cleanup",
      "schedule": "0 2 * * *", "command": "/opt/scripts/cleanup.sh",
      "source": "crontab", "user": "appuser" }
  ],
  "unresolved_connections": [...]
}
```

#### E2. Dépendances par config (pas seulement TCP)

La corrélation croise maintenant 3 sources :
1. **TCP connections** : process A → remote_addr:port matche un listener sur agent B
2. **Config endpoints** : config de process A mentionne host:port qui matche un agent B
3. **Port typing** : même sans matche d'agent, le port indique la technologie (5432=PG, etc.)

Priorité de confiance : `config_file` > `tcp_connection` > `port_match`

#### E3. Commandes custom auto-générées

Pour chaque composant dans le draft, générer aussi des commandes custom :
- Si log_files trouvés : `tail -f {log_path}` (label: "View Logs")
- Si config_files trouvés : `cat {config_path}` (label: "View Config")

### F. Migration `V021__discovery_enriched.sql`

```sql
-- Extend draft components with operational fields
ALTER TABLE discovery_draft_components
    ADD COLUMN check_cmd TEXT,
    ADD COLUMN start_cmd TEXT,
    ADD COLUMN stop_cmd TEXT,
    ADD COLUMN restart_cmd TEXT,
    ADD COLUMN command_confidence VARCHAR(10) DEFAULT 'low',
    ADD COLUMN command_source VARCHAR(30),
    ADD COLUMN config_files JSONB DEFAULT '[]'::jsonb,
    ADD COLUMN log_files JSONB DEFAULT '[]'::jsonb,
    ADD COLUMN matched_service VARCHAR(200);
```

### G. Frontend : Step 4 enrichi

Le wizard Step 4 (Configure) montre maintenant pour chaque composant :
- Nom (éditable)
- Type (dropdown : service, database, cache, queue, proxy, batch, web)
- Commandes check/start/stop (pré-remplis par suggestion, éditables)
  - Badge de confiance (vert=high, jaune=medium, rouge=low)
- Config files détectés (affichage, non éditable)
- Log files détectés (affichage)
- Dépendances : source (TCP vs config_file) avec la clé de config

Section "Scheduled Jobs" : les cron/timers détectés que l'utilisateur peut promouvoir en composant de type "batch" avec check = "dernière exécution < intervalle attendu".

### H. `apply_draft` enrichi

Quand l'utilisateur applique le draft, les composants créés dans la table `components` ont maintenant :
- `check_cmd`, `start_cmd`, `stop_cmd` pré-remplis
- `component_type` correctement typé
- `host` renseigné
- `agent_id` résolu

Et pour chaque composant avec des logs/configs :
- Insertion dans `component_commands` : "View Logs" → `tail -100 {path}`
- Insertion dans `component_links` : "Config" → link_type='documentation'

## Fichiers à modifier

| Fichier | Changement |
|---------|-----------|
| `crates/common/src/types.rs` | +6 structs (ConfigFile, Endpoint, LogFile, ScheduledJob, CommandSuggestion), enrichir DiscoveredProcess |
| `crates/common/src/protocol.rs` | +scheduled_jobs dans DiscoveryReport |
| `crates/agent/src/discovery/mod.rs` | Orchestrer scan enrichi, cross-ref process↔service, command suggestion |
| `crates/agent/src/discovery/linux.rs` | +cwd, +fd scanning, +config parsing, +cron/timer, +service cross-ref |
| `crates/agent/src/discovery/windows.rs` | +service cross-ref commands, +schtasks scanning |
| `crates/backend/src/api/discovery.rs` | Enrichir correlate (configs, commands, deps par config), enrichir apply_draft |
| `crates/backend/src/websocket/mod.rs` | Stocker scheduled_jobs dans report JSONB |
| `migrations/V021__discovery_enriched.sql` | Colonnes opérationnelles sur draft_components |
| `frontend/src/api/discovery.ts` | Types enrichis |
| `frontend/src/pages/DiscoveryPage.tsx` | Step 4 : commandes éditables, configs, logs, batch jobs |

## Vérification

1. `cargo build --workspace`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace` — tests existants + nouveaux tests pour parsing config, cron, command suggestion
4. `cd frontend && npm run build && npm test`
5. Push + CI 14/14 vert
