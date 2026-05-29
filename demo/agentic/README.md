# Agentic demo — real services discovered & mapped by AI

This demo proves the thesis end-to-end: **it's no longer "just an agent on a
box" — it's an agentic solution.** One container runs real services; the
AppControl agent discovers them with zero backend; the AI layer turns that into
a readable architecture map.

## What runs inside the container

| Service | Port | Role in the map |
|---|---|---|
| PostgreSQL | 5432 | Database |
| Redis | 6379 | Cache |
| nginx | 80/443 | Web front |
| `order-api` (Python stand-in) | 8080 | Service — depends on PostgreSQL + Redis |

The `order-api` keeps its `application.yml` open and holds TCP connections to
PostgreSQL and Redis, so the agent detects the dependencies **both** from config
(`spring.datasource.url`) and from live TCP connections.

## Run it

```bash
docker compose -f demo/agentic/docker-compose.yml up --build
```

The logs show two steps:
1. **Discovery** — the agent scans the host (`appcontrol-agent discover`).
2. **AI architect pass** — `appcontrol-ai architect` renders the map.

The container stays up. Re-run the chain anytime:

```bash
docker exec -it appcontrol-agentic-demo bash -c \
  'appcontrol-agent discover --json | appcontrol-ai architect'
```

Inspect the raw discovery JSON:

```bash
docker exec -it appcontrol-agentic-demo cat /tmp/discovery.json
```

## Optional: plug a real model for the L0 naming

By default a deterministic mock names the application groups. To use a sovereign
local model (e.g. Ollama on the host), set in `docker-compose.yml`:

```yaml
environment:
  AI_INFERENCE_MODE: hybrid
  AI_LOCAL_BASE_URL: "http://host.docker.internal:11434/v1"
  AI_LOCAL_MODEL: "qwen2.5:14b"
```

Nothing sensitive leaves the machine: the architect sends only a **redacted,
abstract** summary (roles + technologies, no hosts, no paths, no secrets).

## No Docker? Same thing, locally

```bash
cargo run -p appcontrol-agent -- discover --json | cargo run -p appcontrol-ai -- architect
```
