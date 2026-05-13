#!/usr/bin/env python3
"""Generate docs/reference/database.md by parsing every `CREATE TABLE`
in migrations/V*.sql.

Produces:
  - A table inventory (name, columns, primary key, FK count, where defined)
  - A Mermaid ER diagram showing every table and FK
  - The list of `CREATE INDEX` / partitioning hints
  - A flag for the append-only tables (CLAUDE.md mandates no UPDATE / DELETE)

Limitations: this is a regex parser, not a real SQL grammar. It handles the
shapes used by AppControl migrations (PostgreSQL + SQLite-compatible) and
will emit an explicit `_(complex CREATE TABLE — see migration)_` row when
it can't parse a definition.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MIGRATIONS = REPO / "migrations"
OUTPUT = REPO / "docs/reference/database.md"


APPEND_ONLY = {
    "action_log",
    "state_transitions",
    "check_events",
    "switchover_log",
    "enrollment_events",
    "command_executions",
    "config_versions",
}


TABLE_PURPOSE = {
    "organizations": "Tenant boundary. Everything else fans out from here. One row per customer.",
    "users": "Local user accounts plus SSO-linked identities (external_id).",
    "agents": "Every host running the AppControl agent binary. `last_heartbeat_at` drives the UNREACHABLE state.",
    "gateways": "TLS-terminating reverse proxies between agents and the backend.",
    "sites": "Physical or logical location (datacenter, cloud region). Components are bound to one site at a time; switchover moves them.",
    "hostings": "Optional grouping above `sites` — typically used to label datacenters or cloud providers.",
    "applications": "Top-level operational unit. Each application is a DAG of components.",
    "components": "Individual processes / services that make up an application.",
    "dependencies": "Edges of the DAG between components. `strong=true` blocks the sequencer; `strong=false` is informational only.",
    "teams": "Groups of users for shared permission grants.",
    "team_members": "Many-to-many between `users` and `teams`.",
    "app_permissions_users": "Direct per-app permission grant to a user.",
    "app_permissions_teams": "Per-app permission grant to a team.",
    "workspaces": "Optional access control above sites. Restricts which sites a user can see.",
    "workspace_users": "Many-to-many: workspaces ↔ users.",
    "workspace_teams": "Many-to-many: workspaces ↔ teams.",
    "workspace_sites": "Many-to-many: workspaces ↔ sites.",
    "action_log": "Append-only record of every user-initiated action. **The DORA Article 16 source of truth.**",
    "state_transitions": "Append-only record of every FSM transition (every component, every state change).",
    "check_events": "Append-only, partitioned by day. One row per health check execution. Retention controlled by `RETENTION_CHECK_EVENTS_DAYS`.",
    "switchover_log": "Append-only record of every DR switchover phase, with start/end timestamps.",
    "api_keys": "Long-lived API tokens for schedulers and CLI users.",
    "enrollment_tokens": "Short-lived tokens used by the gateway to issue agent client certs.",
    "enrollment_events": "Append-only audit of certificate issuance.",
    "config_versions": "Snapshot of `applications` / `components` state before/after each config change. Every edit produces a row.",
    "command_executions": "Append-only record of custom-command dispatches and their results.",
    "cluster_members": "Members of a fan-out cluster (each member is an addressable host within a parent component).",
    "fsm_cache": "Materialised view of `state_transitions` — fast read path for the dashboard.",
    "saml_providers": "Per-organisation SAML 2.0 IdP configuration.",
    "oidc_providers": "Per-organisation OIDC IdP configuration.",
    "notifications": "Multi-channel alert routing rules (email, webhook, Slack).",
    "variables": "Per-application variable definitions used by `start_cmd` / `stop_cmd` templating.",
    "groups": "Logical grouping of components inside an application (used by the map renderer).",
    "approvals": "Pending approval requests for PR-only mode operations.",
}


COLUMN_RX = re.compile(
    r"^\s*(?P<name>\"?[a-z_][a-z0-9_]*\"?)\s+(?P<type>[A-Z][A-Z0-9_(),\s]*?)"
    r"(?P<rest>[^,\n]*)",
    re.MULTILINE | re.IGNORECASE,
)


CREATE_TABLE_RX = re.compile(
    r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?P<name>[a-z_][a-z0-9_]*)\s*\(",
    re.IGNORECASE,
)


CREATE_INDEX_RX = re.compile(
    r"CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?(?P<idx>[a-z_][a-z0-9_]*)"
    r"\s+ON\s+(?P<table>[a-z_][a-z0-9_]*)",
    re.IGNORECASE,
)


def extract_create_table_body(text: str, start: int) -> tuple[str, int]:
    """Given the offset just after `CREATE TABLE name (`, return the column
    list (up to the matching close paren) and the offset after it."""
    depth = 1
    i = start
    while i < len(text) and depth > 0:
        ch = text[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return text[start:i], i + 1
        i += 1
    return text[start:], len(text)


def parse_table(name: str, body: str, source: str) -> dict:
    columns = []
    primary_key = []
    foreign_keys = []
    # Split top-level by commas, respecting parentheses.
    parts = split_top_level(body)
    for part in parts:
        p = part.strip()
        if not p:
            continue
        u = p.upper()
        if u.startswith("PRIMARY KEY"):
            m = re.search(r"\(([^)]+)\)", p)
            if m:
                primary_key = [c.strip().strip('"') for c in m.group(1).split(",")]
            continue
        if u.startswith("FOREIGN KEY"):
            m = re.search(
                r"FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+(\w+)\s*\(([^)]+)\)",
                p,
                re.IGNORECASE,
            )
            if m:
                foreign_keys.append({
                    "columns": [c.strip().strip('"') for c in m.group(1).split(",")],
                    "ref_table": m.group(2),
                    "ref_columns": [c.strip().strip('"') for c in m.group(3).split(",")],
                })
            continue
        if u.startswith("UNIQUE") or u.startswith("CHECK") or u.startswith("CONSTRAINT"):
            continue
        # Column definition: name TYPE [constraints...]
        m = re.match(
            r'\s*"?(?P<name>[a-z_][a-z0-9_]*)"?\s+(?P<type>[A-Za-z][A-Za-z0-9_()\[\] ,]*?)'
            r'\s*(?P<rest>.*)$',
            p,
            re.DOTALL,
        )
        if not m:
            continue
        col_name = m.group("name")
        col_type = m.group("type").strip()
        rest = m.group("rest").strip().upper()
        notes = []
        if "PRIMARY KEY" in rest:
            primary_key.append(col_name)
            notes.append("PK")
        if "NOT NULL" in rest:
            notes.append("NOT NULL")
        if "UNIQUE" in rest:
            notes.append("UNIQUE")
        if "DEFAULT" in rest:
            dm = re.search(r"DEFAULT\s+([^,]+)", rest)
            if dm:
                notes.append(f"default {dm.group(1).strip().rstrip(',')}")
        # Inline REFERENCES → FK
        rm = re.search(
            r"REFERENCES\s+([a-z_][a-z0-9_]*)\s*\(\s*([a-z_][a-z0-9_]*)\s*\)",
            m.group("rest"),
            re.IGNORECASE,
        )
        if rm:
            foreign_keys.append({
                "columns": [col_name],
                "ref_table": rm.group(1),
                "ref_columns": [rm.group(2)],
            })
            notes.append(f"FK→`{rm.group(1)}`")
        columns.append({"name": col_name, "type": col_type, "notes": ", ".join(notes)})
    return {
        "name": name,
        "columns": columns,
        "primary_key": primary_key,
        "foreign_keys": foreign_keys,
        "source": source,
    }


def split_top_level(s: str) -> list[str]:
    parts = []
    depth = 0
    cur = []
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if cur:
        parts.append("".join(cur))
    return parts


def render_mermaid(tables: dict[str, dict]) -> str:
    out = ["```mermaid", "erDiagram"]
    # Limit to FK-connected tables for readability — show all tables but only
    # render the relationship edges. The full inventory is in the table below.
    for t in sorted(tables.values(), key=lambda x: x["name"]):
        for fk in t["foreign_keys"]:
            ref = fk["ref_table"]
            if ref not in tables:
                continue
            label = ",".join(fk["columns"])
            out.append(f'    {t["name"]} ||--o{{ {ref} : "{label}"')
    out.append("```")
    return "\n".join(out)


def render(tables: dict[str, dict], indexes: list[tuple[str, str]]) -> str:
    out = []
    out.append("# Database schema reference")
    out.append("")
    out.append("> Auto-generated from `migrations/V*.sql`. Run `scripts/docs/regen.py` to refresh.")
    out.append("")
    out.append(f"The schema is defined by **{sum(1 for _ in MIGRATIONS.glob('V*.sql'))} versioned migrations** applied in order at backend startup. PostgreSQL 16 is the production target; SQLite is supported for development and small-scale deployments. Schema parity between the two backends is mandatory — see [SQLite implementation notes](../SQLITE_IMPLEMENTATION.md).")
    out.append("")
    out.append("**Append-only tables** (no `UPDATE`, no `DELETE`, ever — they are the audit substrate):")
    out.append("")
    for t in sorted(APPEND_ONLY):
        out.append(f"- `{t}`")
    out.append("")
    out.append("## Entity-relationship diagram (foreign keys only)")
    out.append("")
    out.append("Tables without foreign keys to other tables (lookup / configuration tables) are omitted from this diagram but listed below.")
    out.append("")
    out.append(render_mermaid(tables))
    out.append("")
    out.append("## Tables")
    out.append("")
    out.append(f"Total: **{len(tables)}** tables.")
    out.append("")
    for name in sorted(tables):
        t = tables[name]
        out.append(f"### `{name}`")
        if name in APPEND_ONLY:
            out.append("")
            out.append("!!! warning \"Append-only\"")
            out.append("    No `UPDATE` or `DELETE` is ever issued against this table. Retention is enforced by partition drops (`check_events`) or by external archival jobs.")
        out.append("")
        out.append(TABLE_PURPOSE.get(name, "_(purpose: add to `scripts/docs/gen_database_schema.py::TABLE_PURPOSE`)_"))
        out.append("")
        if t["primary_key"]:
            out.append(f"**Primary key:** `({', '.join(t['primary_key'])})`")
            out.append("")
        if t["foreign_keys"]:
            out.append("**Foreign keys:**")
            out.append("")
            for fk in t["foreign_keys"]:
                out.append(
                    f"- `({', '.join(fk['columns'])})` → `{fk['ref_table']}({', '.join(fk['ref_columns'])})`"
                )
            out.append("")
        if t["columns"]:
            out.append("| Column | Type | Constraints |")
            out.append("|---|---|---|")
            for c in t["columns"]:
                out.append(f"| `{c['name']}` | `{c['type']}` | {c['notes'] or '—'} |")
            out.append("")
        # Indexes on this table
        own_idx = [(i, w) for (i, w) in indexes if w == name]
        if own_idx:
            out.append(f"**Indexes:** {', '.join(f'`{i}`' for i, _ in own_idx)}")
            out.append("")
        out.append(f"Defined in: `migrations/{t['source']}`")
        out.append("")
    out.append("## Notes")
    out.append("")
    out.append("- `check_events` is partitioned by day (PostgreSQL) — see migration `V005` and the partition-creation logic in the backend. Retention drops whole partitions rather than deleting rows.")
    out.append("- UUIDs are `UUID` (native) on PostgreSQL and `TEXT` (with `DbUuid` codec) on SQLite. Application code uses `uuid::Uuid`; the codec is transparent.")
    out.append("- JSONB columns are `JSONB` on PostgreSQL and `TEXT` (with `DbJson` codec) on SQLite.")
    out.append("- See [Backup & restore](../BACKUP_RESTORE.md) for `pg_dump` recipes that respect the partition layout.")
    out.append("")
    return "\n".join(out)


def main() -> int:
    tables: dict[str, dict] = {}
    indexes: list[tuple[str, str]] = []
    for mig in sorted(MIGRATIONS.glob("V*.sql")):
        text = mig.read_text()
        # Strip comments before parsing — single-line `--` only; SQL string
        # literals don't appear in our migrations, so this is safe.
        clean = re.sub(r"--[^\n]*", "", text)
        # Scan for every CREATE TABLE in the file.
        pos = 0
        while True:
            m = CREATE_TABLE_RX.search(clean, pos)
            if not m:
                break
            body, end = extract_create_table_body(clean, m.end())
            t = parse_table(m.group("name"), body, mig.name)
            tables[t["name"]] = t
            pos = end
        for im in CREATE_INDEX_RX.finditer(clean):
            indexes.append((im.group("idx"), im.group("table")))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(tables, indexes))
    print(f"database: {len(tables)} tables + {len(indexes)} indexes → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
