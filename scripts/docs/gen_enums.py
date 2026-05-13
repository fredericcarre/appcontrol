#!/usr/bin/env python3
"""Generate docs/reference/enums.md from crates/common/src/types.rs.

Surfaces the enums users encounter in the API and UI:
  - PermissionLevel (5 levels + None)
  - ComponentType
  - CheckType
  - CheckStatus
  - DiagnosticRecommendation
  - ClusterMode, ClusterHealthPolicy
  - SwitchoverPhase, SwitchoverMode
  - OrgRole
  - UpdateStatus

For each, parses the doc comment block above the enum and the variants, and
emits a table with the wire-serialised name (from `#[serde(rename_all = …)]`).
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "crates/common/src/types.rs"
OUTPUT = REPO / "docs/reference/enums.md"


WIRE_ENUMS = [
    "PermissionLevel",
    "ComponentState",
    "ComponentType",
    "CheckType",
    "CheckStatus",
    "DiagnosticRecommendation",
    "ClusterMode",
    "ClusterHealthPolicy",
    "SwitchoverPhase",
    "SwitchoverMode",
    "OrgRole",
    "UpdateStatus",
]


def apply_rename(variant: str, rule: str | None) -> str:
    if not rule:
        return variant
    if rule == "lowercase":
        return variant.lower()
    if rule == "UPPERCASE":
        return variant.upper()
    if rule == "snake_case":
        return re.sub(r"(?<!^)(?=[A-Z])", "_", variant).lower()
    if rule == "SCREAMING_SNAKE_CASE":
        return re.sub(r"(?<!^)(?=[A-Z])", "_", variant).upper()
    if rule == "PascalCase":
        return variant  # already
    return variant


ENUM_RX = re.compile(
    r"(?P<doc>(?:^\s*///[^\n]*\n)*)"
    r"(?P<attrs>(?:^\s*#\[[^\]]+\]\s*\n)+)"
    r"pub enum (?P<name>[A-Z][A-Za-z0-9_]+)\s*\{(?P<body>[^}]*)\}",
    re.MULTILINE,
)


def parse_enum(text: str, name: str) -> dict | None:
    for m in ENUM_RX.finditer(text):
        if m.group("name") != name:
            continue
        attrs = m.group("attrs")
        # Look for both `#[serde(rename_all = ...)]` and `#[strum(...)]` — the
        # serde one wins on the wire.
        rule = None
        sm = re.search(r'#\[serde\([^\]]*rename_all\s*=\s*"([^"]+)"', attrs)
        if sm:
            rule = sm.group(1)
        doc = " ".join(
            line.strip().lstrip("/").strip()
            for line in m.group("doc").splitlines()
            if line.strip().startswith("///")
        )
        body = m.group("body")
        # Each variant line may have a /// above it.
        variants = []
        lines = body.splitlines()
        pending = []
        for line in lines:
            s = line.strip()
            if not s or s.startswith("//"):
                if s.startswith("///"):
                    pending.append(s[3:].strip())
                continue
            if s.startswith("#["):
                continue
            # Variant name: optionally followed by `,` or `= N,`
            vm = re.match(r"(?P<name>[A-Z][A-Za-z0-9_]*)\s*(?:=\s*(?P<value>\d+))?\s*,?", s)
            if not vm:
                pending = []
                continue
            variants.append({
                "name": vm.group("name"),
                "value": vm.group("value"),
                "doc": " ".join(pending),
                "wire": apply_rename(vm.group("name"), rule),
            })
            pending = []
        return {"name": name, "doc": doc, "rule": rule, "variants": variants}
    return None


# Editor notes — short, business-relevant explanations of each enum to layer
# on top of the doc comments mined from source.
ENUM_INTRO = {
    "PermissionLevel": "Granular per-application permission. Ordered: `None < View < Operate < Edit < Manage < Owner`. Effective permission is the maximum of direct grant, every team grant, and the user's organisation role.",
    "ComponentState": "The 8 FSM states a component can be in. See the [FSM reference](fsm.md) for the transition rules.",
    "ComponentType": "Catalogue tag used by the map renderer to pick an icon and by the import wizard to group components.",
    "CheckType": "The four kinds of agent-side checks. Only `Health` drives the FSM; the others are informational and surface in the diagnostic page.",
    "CheckStatus": "Outcome of a diagnostic-level check. Feeds the diagnostic-recommendation matrix.",
    "DiagnosticRecommendation": "Output of `core::diagnostic::ComponentDiagnosis` — what the system suggests doing about a degraded component.",
    "ClusterMode": "How a multi-host component is represented. `Aggregate` trusts an external aggregator (F5, Oracle SCAN). `FanOut` makes each member a first-class entity with its own FSM and history.",
    "ClusterHealthPolicy": "When `cluster_mode = FanOut`, how to derive the parent component's state from member states.",
    "SwitchoverPhase": "The six phases of a DR switchover, executed sequentially with rollback available after each one. See the [User Guide](../USER_GUIDE.md#dr-site-switchover).",
    "SwitchoverMode": "How aggressively to fail over. `Full` moves every component; `Selective` moves a chosen subset; `Progressive` moves component groups in sequence with explicit go/no-go between groups.",
    "OrgRole": "Organisation-wide role. `Admin` is an implicit `Owner` on every application; the other roles imply nothing — application-level permissions are still required to operate.",
    "UpdateStatus": "Status of an in-progress agent self-update (air-gap bundle distribution).",
}


def render(parsed: list[dict]) -> str:
    out = []
    out.append("# Enum reference")
    out.append("")
    out.append("> Auto-generated from `crates/common/src/types.rs`. Run `scripts/docs/regen.py` to refresh.")
    out.append("")
    out.append("These are the typed enums exposed across the REST API, the WebSocket protocol, the CLI, and the database. The wire format column shows the exact string a client must send or expect.")
    out.append("")
    for e in parsed:
        out.append(f"## `{e['name']}`")
        out.append("")
        if e["name"] in ENUM_INTRO:
            out.append(ENUM_INTRO[e["name"]])
            out.append("")
        if e["doc"]:
            out.append(f"_Source comment: {e['doc']}_")
            out.append("")
        out.append("| Variant | Wire format | Numeric value | Description |")
        out.append("|---|---|---|---|")
        for v in e["variants"]:
            value = v["value"] if v["value"] else "—"
            doc = v["doc"] or "—"
            out.append(f"| `{v['name']}` | `\"{v['wire']}\"` | {value} | {doc} |")
        out.append("")
    return "\n".join(out)


def main() -> int:
    text = SOURCE.read_text()
    parsed = []
    missing = []
    for name in WIRE_ENUMS:
        e = parse_enum(text, name)
        if e is None:
            missing.append(name)
            continue
        parsed.append(e)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(parsed))
    print(f"enums: {len(parsed)} enums rendered → {OUTPUT.relative_to(REPO)}" + (f" (missing: {missing})" if missing else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
