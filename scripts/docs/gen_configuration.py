#!/usr/bin/env python3
"""Generate docs/reference/configuration.md by scanning the config structs
of every component (backend, agent, gateway).

Strategy:
  - For the backend, parse `crates/backend/src/config.rs` — every `AppConfig`
    field has a doc comment describing it, and the `from_env()` method tells
    us the env-var name and default.
  - For the agent, parse `crates/agent/src/config.rs` for the YAML config
    plus the env-var overrides in `Config::from_yaml_or_env()`.
  - For the gateway, parse `crates/gateway/src/main.rs` — gateway uses CLI
    flags, not env vars; we surface the flags as the "config reference".

The generator is regex-based; if a field has no doc comment the row is still
emitted with a `_(undocumented)_` description so the omission shows up in the
rendered table.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BACKEND = REPO / "crates/backend/src/config.rs"
AGENT = REPO / "crates/agent/src/config.rs"
GATEWAY = REPO / "crates/gateway/src/main.rs"
OUTPUT = REPO / "docs/reference/configuration.md"


def parse_env_vars(source: Path) -> list[dict]:
    """Find every `std::env::var("FOO")` site and pair it with the surrounding
    default value, returning a list of {name, default, line}."""
    text = source.read_text()
    rows = []
    # Pattern A: std::env::var("X").unwrap_or_else(|_| "default".to_string())
    a = re.compile(
        r'std::env::var\("(?P<name>[A-Z][A-Z0-9_]+)"\)'
        r'(?:\.ok\(\))?'
        r'(?:'
        r'\.unwrap_or_else\(\|_\|\s*"(?P<default_str>[^"]*)"\.to_string\(\)\)'
        r'|'
        r'\.unwrap_or(?:_else)?\(\|?_?\|?\s*(?P<default_expr>[^)]+)\)'
        r')?',
        re.MULTILINE,
    )
    seen = set()
    for m in a.finditer(text):
        name = m.group("name")
        if name in seen:
            continue
        seen.add(name)
        default = m.group("default_str")
        if default is None:
            default_expr = m.group("default_expr")
            if default_expr:
                default = default_expr.strip().rstrip(",")
            else:
                default = "_(none — unset means feature off / required in production)_"
        else:
            default = f"`{default}`"
        line = text.count("\n", 0, m.start()) + 1
        rows.append({
            "name": name,
            "default": default,
            "line": line,
        })
    return rows


# Annotations the code itself does not carry. Edit here when adding a new env var.
ANNOTATIONS: dict[str, dict[str, str]] = {
    "APP_ENV": {
        "desc": "Application environment. `production` makes the backend panic on insecure `JWT_SECRET` and on missing `DATABASE_URL`.",
        "values": "`development` | `staging` | `production`",
    },
    "PORT": {
        "desc": "HTTP port the backend listens on.",
        "values": "1024–65535",
    },
    "DATABASE_URL": {
        "desc": "Postgres or SQLite connection string. Required in production (panics if unset). Default in dev: `postgresql://appcontrol:appcontrol@localhost:5432/appcontrol` (postgres build) or `sqlite:./appcontrol.db` (sqlite build).",
        "values": "`postgresql://USER:PASS@HOST:PORT/DB` or `sqlite:./path/to/file.db`",
    },
    "SQLITE_PATH": {
        "desc": "Path to the SQLite database file. Only consulted when `DATABASE_URL` is unset and the binary was built with the `sqlite` feature.",
        "values": "absolute or relative path",
    },
    "JWT_SECRET": {
        "desc": "HMAC secret for JWT signing. Required in production; panics if shorter than 32 chars or contains `dev` / `change`. Generate with `openssl rand -base64 48`.",
        "values": "≥32 random chars",
    },
    "JWT_ISSUER": {
        "desc": "`iss` claim written into every issued JWT.",
        "values": "string",
    },
    "CORS_ORIGINS": {
        "desc": "Comma-separated list of origins permitted to call the API from a browser. Empty in production logs a warning — cross-origin browser calls will be rejected.",
        "values": "`https://app.example.com,https://admin.example.com`",
    },
    "LOG_FORMAT": {
        "desc": "Log output format. Use `json` for SIEM ingestion.",
        "values": "`text` | `json`",
    },
    "DB_POOL_SIZE": {
        "desc": "Maximum number of database connections in the pool.",
        "values": "integer ≥ 1",
    },
    "DB_IDLE_TIMEOUT_SECS": {
        "desc": "Idle connections are dropped after this many seconds.",
        "values": "seconds",
    },
    "DB_CONNECT_TIMEOUT_SECS": {
        "desc": "Maximum time `pool.acquire()` waits for a free connection before returning a database error.",
        "values": "seconds",
    },
    "SHUTDOWN_TIMEOUT_SECS": {
        "desc": "How long the backend waits for in-flight requests to drain on SIGTERM before forcing exit.",
        "values": "seconds",
    },
    "RATE_LIMIT_AUTH": {
        "desc": "Requests per minute on auth endpoints (login, refresh) per source IP.",
        "values": "integer",
    },
    "RATE_LIMIT_OPERATIONS": {
        "desc": "Operations (start / stop / switchover) per minute per user.",
        "values": "integer",
    },
    "RATE_LIMIT_READS": {
        "desc": "Read requests per minute per user — applies to `/apps`, `/components`, status endpoints.",
        "values": "integer",
    },
    "HA_MODE": {
        "desc": "When `true`, rate limiting and lock coordination use PostgreSQL instead of in-memory state. Set this on every backend replica when running multiple instances behind a load balancer.",
        "values": "`true` | `false`",
    },
    "RETENTION_ACTION_LOG_DAYS": {
        "desc": "Auto-prune `action_log` rows older than this. `0` keeps everything forever (recommended for DORA-style audit requirements — set to 1825 = 5 years if you must prune).",
        "values": "days; `0` = unlimited",
    },
    "RETENTION_CHECK_EVENTS_DAYS": {
        "desc": "Drop `check_events` partitions older than this many days. `0` keeps everything. Partitions are dropped, not deleted row-by-row.",
        "values": "days; `0` = unlimited",
    },
    "PUBLIC_GATEWAY_URL": {
        "desc": "Public WSS URL the frontend embeds in agent-enrollment instructions. Leave unset to derive from `window.location.host:4443`.",
        "values": "`wss://gateway.example.com:4443`",
    },
    "PUBLIC_BACKEND_URL": {
        "desc": "Public backend URL the frontend uses for the gateway-to-backend WebSocket. Leave unset to derive from `window.location`.",
        "values": "`wss://backend.example.com/ws/gateway`",
    },
    "SEED_ENABLED": {
        "desc": "When `true` and no users exist at boot, create an initial organisation + admin user from the `SEED_*` variables. Disable in production after first boot.",
        "values": "`true` | `false`",
    },
    "SEED_ADMIN_EMAIL": {"desc": "Email of the seeded admin user.", "values": "email"},
    "SEED_ADMIN_PASSWORD": {"desc": "Plaintext password of the seeded admin user; bcrypt-hashed before storage. Change it immediately after first login.", "values": "string"},
    "SEED_ADMIN_DISPLAY_NAME": {"desc": "Display name for the seeded admin.", "values": "string"},
    "SEED_ORG_NAME": {"desc": "Name of the seeded organisation.", "values": "string"},
    "SEED_ORG_SLUG": {"desc": "URL-safe slug for the seeded organisation.", "values": "lowercase alphanumeric"},
    # Agent env vars
    "AGENT_ID": {"desc": "Agent UUID, or `auto` to generate one on first start. Once set, the file `data_dir/agent.id` is the source of truth — env var is only consulted on a fresh install.", "values": "`auto` | UUID"},
    "GATEWAY_URL": {"desc": "Single gateway URL. Legacy; prefer `GATEWAY_URLS` for failover.", "values": "`wss://gateway:4443`"},
    "GATEWAY_URLS": {"desc": "Comma-separated list of gateway URLs for failover. The agent walks them per the `failover_strategy` (default `ordered`) and re-tries the primary every `primary_retry_secs`.", "values": "`wss://gw1:4443,wss://gw2:4443`"},
    "GATEWAY_RECONNECT_SECS": {"desc": "Sleep between gateway-reconnect attempts.", "values": "seconds"},
    "AGENT_MODE": {"desc": "Operating mode. `advisory` runs checks and reports state but refuses every start/stop/rebuild dispatched from the backend.", "values": "`active` | `advisory`"},
    "DATA_DIR": {"desc": "Where the agent stores its buffer database, enrolment cert, and `agent.id`. Defaults to `/var/lib/appcontrol` (Unix) or `C:\\\\ProgramData\\\\AppControl` (Windows).", "values": "directory path"},
    "TLS_ENABLED": {"desc": "Enable mTLS for the gateway connection. Required in production.", "values": "`true` | `false`"},
    "TLS_CERT_FILE": {"desc": "Agent's client certificate.", "values": "PEM path"},
    "TLS_KEY_FILE": {"desc": "Agent's client private key.", "values": "PEM path"},
    "TLS_CA_FILE": {"desc": "CA bundle used to verify the gateway's server cert.", "values": "PEM path"},
    "TLS_INSECURE": {"desc": "Skip TLS verification. **Never** enable in production.", "values": "`true` | `false`"},
}


def render_table(component: str, source: Path, vars_: list[dict], note: str | None = None) -> list[str]:
    out: list[str] = []
    out.append(f"### {component}")
    out.append("")
    if note:
        out.append(note)
        out.append("")
    out.append(f"Source: `{source.relative_to(REPO)}`. {len(vars_)} variables.")
    out.append("")
    out.append("| Env var | Default | Allowed values | Description |")
    out.append("|---|---|---|---|")
    for v in sorted(vars_, key=lambda r: r["name"]):
        meta = ANNOTATIONS.get(v["name"], {"desc": "_(undocumented — add to `scripts/docs/gen_configuration.py::ANNOTATIONS`)_", "values": ""})
        out.append(
            f"| `{v['name']}` | {v['default']} | {meta.get('values', '')} | {meta.get('desc', '')} |"
        )
    out.append("")
    return out


def render(backend: list[dict], agent: list[dict]) -> str:
    out = []
    out.append("# Configuration reference")
    out.append("")
    out.append("> Auto-generated by scanning `crates/{backend,agent}/src/config.rs`. Run `scripts/docs/regen.py` to refresh.")
    out.append("")
    out.append("Configuration comes from environment variables (backend, agent) or CLI flags (gateway). Settings exposed through the UI (organisation `heartbeat_timeout_seconds`, per-app retention overrides, etc.) live in the database and are not covered here — see the [User Guide](../USER_GUIDE.md#settings--administration).")
    out.append("")
    out.append("Quick start for a production-ready deployment:")
    out.append("")
    out.append("```bash")
    out.append("export APP_ENV=production")
    out.append("export JWT_SECRET=$(openssl rand -base64 48)")
    out.append("export DATABASE_URL=postgresql://appcontrol:STRONG_PASS@db.internal:5432/appcontrol")
    out.append("export CORS_ORIGINS=https://appcontrol.example.com")
    out.append("export HA_MODE=true                       # only if running multiple backend replicas")
    out.append("export LOG_FORMAT=json                    # for SIEM ingestion")
    out.append("export RETENTION_ACTION_LOG_DAYS=1825     # 5 years — DORA")
    out.append("export RETENTION_CHECK_EVENTS_DAYS=365    # 1 year")
    out.append("```")
    out.append("")
    out.extend(render_table("Backend", BACKEND, backend))
    out.extend(render_table(
        "Agent",
        AGENT,
        agent,
        note="Agent configuration is read from a YAML file (default location: `/etc/appcontrol/agent.yaml` on Unix, `C:\\\\ProgramData\\\\AppControl\\\\agent.yaml` on Windows). Every key can be overridden by an environment variable. The variables below are the env-override surface.",
    ))
    out.append("### Gateway")
    out.append("")
    out.append("The gateway is configured via CLI flags rather than env vars. The authoritative list is `crates/gateway/src/main.rs`. The common flags:")
    out.append("")
    out.append("| Flag | Default | Description |")
    out.append("|---|---|---|")
    out.append("| `--bind` | `0.0.0.0:4443` | Address the gateway listens on for inbound agent WebSocket connections. |")
    out.append("| `--backend-url` | `wss://localhost:8080/ws/gateway` | Backend endpoint the gateway dials. |")
    out.append("| `--cert` | `/etc/appcontrol/gateway.crt` | Server certificate presented to agents. |")
    out.append("| `--key` | `/etc/appcontrol/gateway.key` | Server private key. |")
    out.append("| `--ca` | `/etc/appcontrol/ca.crt` | CA bundle used to verify agent client certs (mTLS). |")
    out.append("| `--name` | hostname | Gateway display name, surfaced in the UI. |")
    out.append("| `--rate-limit-msgs-per-sec` | `200` | Per-agent inbound message rate limit. |")
    out.append("")
    out.append("Run `gateway --help` against the binary you ship to get the version-specific flag list.")
    out.append("")
    out.append("## See also")
    out.append("")
    out.append("- [Hardening checklist](../HARDENING.md) — every variable that matters for production security")
    out.append("- [Production deployment](../PRODUCTION_DEPLOYMENT.md) — full Helm + OpenShift recipes")
    out.append("- [High availability](../HIGH_AVAILABILITY.md) — the `HA_MODE` variable in context")
    out.append("")
    return "\n".join(out)


def main() -> int:
    backend = parse_env_vars(BACKEND)
    agent = parse_env_vars(AGENT)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(backend, agent))
    print(f"configuration: backend {len(backend)} + agent {len(agent)} env vars → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
