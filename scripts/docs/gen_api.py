#!/usr/bin/env python3
"""Generate docs/reference/api.md from crates/backend/openapi.json.

We render the spec as readable markdown:
  - Tag-grouped endpoint listing
  - One H3 per (METHOD path) with summary, parameters, request body schema,
    response codes
  - Auth scheme summary
  - Servers / base URL

The spec is the source of truth; this script does NOT augment it. If a
schema is missing from the spec, it is missing from the rendered doc — the
fix is to fill in `openapi.json`, not to patch the renderer.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SPEC = REPO / "crates/backend/openapi.json"
OUTPUT = REPO / "docs/reference/api.md"


def render_param(p: dict) -> str:
    name = p.get("name", "?")
    where = p.get("in", "query")
    required = "**required**" if p.get("required") else "optional"
    desc = p.get("description", "")
    schema = p.get("schema", {})
    type_ = schema.get("type", "any")
    return f"- `{name}` *({where}, {type_}, {required})* — {desc}"


def render_responses(responses: dict) -> str:
    if not responses:
        return "_(no responses documented)_"
    lines = []
    for code in sorted(responses.keys()):
        body = responses[code]
        desc = body.get("description", "")
        lines.append(f"- **{code}** — {desc}")
    return "\n".join(lines)


def render_endpoint(method: str, path: str, op: dict) -> list[str]:
    out = []
    summary = op.get("summary", "")
    out.append(f"### `{method.upper()} {path}`")
    out.append("")
    if summary:
        out.append(f"_{summary}_")
        out.append("")
    if op.get("description"):
        out.append(op["description"])
        out.append("")
    params = op.get("parameters", [])
    if params:
        out.append("**Parameters**")
        out.append("")
        for p in params:
            out.append(render_param(p))
        out.append("")
    rb = op.get("requestBody")
    if rb:
        out.append("**Request body**")
        out.append("")
        content = rb.get("content", {})
        for ctype, body in content.items():
            out.append(f"_Content-Type: `{ctype}`_")
            schema = body.get("schema")
            if schema:
                out.append("")
                out.append("```json")
                out.append(json.dumps(schema, indent=2))
                out.append("```")
            out.append("")
    out.append("**Responses**")
    out.append("")
    out.append(render_responses(op.get("responses", {})))
    out.append("")
    return out


def render(spec: dict) -> str:
    info = spec.get("info", {})
    servers = spec.get("servers", [{"url": "/api/v1"}])
    paths = spec.get("paths", {})
    out = []
    out.append("# REST API reference")
    out.append("")
    out.append("> Auto-generated from `crates/backend/openapi.json`. Run `scripts/docs/regen.py` to refresh. The same spec is served at `GET /api/v1/openapi.json` from a running backend — load it into Postman, Insomnia, or Swagger UI for an interactive view.")
    out.append("")
    out.append(f"**API version:** `{info.get('version', '?')}`")
    out.append("")
    out.append(f"**Base URL:** `{servers[0].get('url', '/api/v1')}` (relative to the backend host)")
    out.append("")
    if info.get("description"):
        out.append(info["description"])
        out.append("")
    out.append("## Authentication")
    out.append("")
    out.append("Every endpoint (except `/health` and `/ready`) requires authentication. Two schemes are supported:")
    out.append("")
    out.append("- **JWT bearer** — for browser sessions. Obtained from `/api/v1/auth/login` (local), the OIDC callback, or the SAML assertion consumer. Pass it as `Authorization: Bearer <jwt>`.")
    out.append("- **API key** — for schedulers, CI, and the `appctl` CLI. Created in the UI under *Settings → API Keys*. Pass it as `Authorization: Bearer <key>` (same header; the backend tries both schemes).")
    out.append("")
    out.append("All API key activity is recorded in `action_log` with `actor_type='api_key'`.")
    out.append("")
    out.append("## Endpoints")
    out.append("")
    # Group by tag for a navigable structure.
    by_tag: dict[str, list[tuple[str, str, dict]]] = {}
    for path, methods in paths.items():
        for method, op in methods.items():
            if method.lower() not in {"get", "post", "put", "patch", "delete"}:
                continue
            tags = op.get("tags", ["Untagged"])
            for tag in tags:
                by_tag.setdefault(tag, []).append((method, path, op))
    for tag in sorted(by_tag):
        out.append(f"### {tag}")
        out.append("")
        # Index for the tag
        for method, path, _ in sorted(by_tag[tag], key=lambda x: (x[1], x[0])):
            anchor = f"{method}-{path}".lower().replace("/", "").replace("{", "").replace("}", "").replace(" ", "-")
            out.append(f"- [`{method.upper()} {path}`](#{anchor})")
        out.append("")
    out.append("## Endpoint detail")
    out.append("")
    for tag in sorted(by_tag):
        out.append(f"### Tag — {tag}")
        out.append("")
        for method, path, op in sorted(by_tag[tag], key=lambda x: (x[1], x[0])):
            out.extend(render_endpoint(method, path, op))
    out.append("## Error responses")
    out.append("")
    out.append("Every error response shares this body shape:")
    out.append("")
    out.append("```json")
    out.append('{ "error": "<machine-readable code>", "message": "<human-readable>" }')
    out.append("```")
    out.append("")
    out.append("The full mapping of `error` codes to HTTP statuses is in the [error reference](errors.md).")
    out.append("")
    out.append("## See also")
    out.append("")
    out.append("- [Error reference](errors.md) — HTTP status mapping and retry guidance")
    out.append("- [CLI reference](cli.md) — `appctl` is a thin wrapper around this API")
    out.append("- [Integration cookbook](../INTEGRATION_COOKBOOK.md) — Control-M / AutoSys / scheduler recipes")
    out.append("")
    return "\n".join(out)


def main() -> int:
    spec = json.loads(SPEC.read_text())
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(spec))
    n_endpoints = sum(
        1
        for path, methods in spec.get("paths", {}).items()
        for method in methods
        if method.lower() in {"get", "post", "put", "patch", "delete"}
    )
    print(f"api: {n_endpoints} endpoints across {len(spec.get('paths', {}))} paths → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
