#!/usr/bin/env python3
"""Generate docs/reference/fsm.md from crates/common/src/fsm.rs + types.rs.

Produces:
  - The list of ComponentState variants and what each means
  - A Mermaid state diagram of all valid transitions
  - A transition matrix (from × to) marking valid cells
  - The check-result decision table (next_state_from_check)

Source of truth: crates/common/src/fsm.rs (`is_valid_transition` and
`next_state_from_check`) plus the doc comments above them.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
FSM = REPO / "crates/common/src/fsm.rs"
TYPES = REPO / "crates/common/src/types.rs"
OUTPUT = REPO / "docs/reference/fsm.md"

STATES = [
    "Unknown",
    "Running",
    "Degraded",
    "Failed",
    "Stopped",
    "Starting",
    "Stopping",
    "Unreachable",
]

STATE_MEANING = {
    "Unknown": "The component has been registered but no health check has reported back yet. The next check determines the state.",
    "Running": "Latest health check returned exit 0. The component is considered operational.",
    "Degraded": "Reserved state for business-logic-defined partial health. Not currently entered by `next_state_from_check`; reachable only via explicit FSM driver code.",
    "Failed": "Latest health check returned a non-zero exit code while the component was `RUNNING` or `DEGRADED`, **or** the sequencer's start timeout elapsed before the process came up.",
    "Stopped": "Stop sequence completed successfully — the agent confirmed the process is gone (check returns non-zero). Also the initial state for components on a freshly enrolled agent.",
    "Starting": "Start command has been dispatched. The component stays in `STARTING` until the next successful health check (`→ RUNNING`) or the sequencer timeout (`→ FAILED`). Non-zero check exits during `STARTING` do **not** transition: the start command is detached and may take time.",
    "Stopping": "Stop command has been dispatched. The component stays in `STOPPING` until the agent confirms the process is gone (`→ STOPPED`). A zero exit during `STOPPING` keeps the component in `STOPPING`.",
    "Unreachable": "The agent hosting the component has missed `heartbeat_timeout_seconds` (default 180s, configurable per organisation). All in-flight states (RUNNING / DEGRADED / STARTING) collapse to `UNREACHABLE`. `STOPPED` and `STOPPING` are preserved because they are intentional.",
}


def parse_valid_transitions() -> list[tuple[str, str]]:
    """Extract valid (from, to) pairs from is_valid_transition()."""
    text = FSM.read_text()
    pairs: list[tuple[str, str]] = []

    # 1. The wildcard rule: "Any state → Unreachable"
    for s in STATES:
        if s != "Unreachable":
            pairs.append((s, "Unreachable"))
    # 2. The wildcard rule: "Unreachable → any state"
    for s in STATES:
        if s != "Unreachable":
            pairs.append(("Unreachable", s))

    # 3. The explicit matches!() block.
    block = re.search(r"matches!\(\s*\(from,\s*to\)\s*,\s*(.*?)\)", text, re.DOTALL)
    if not block:
        raise RuntimeError("is_valid_transition: matches!() block not found")
    body = block.group(1)
    # Each (From, To) pair. Strip comments first.
    body = re.sub(r"//[^\n]*", "", body)
    for m in re.finditer(r"\(\s*(\w+)\s*,\s*(\w+)\s*\)", body):
        pairs.append((m.group(1), m.group(2)))

    # Dedupe while preserving order.
    seen = set()
    out = []
    for p in pairs:
        if p not in seen:
            seen.add(p)
            out.append(p)
    return out


def parse_check_decisions() -> list[tuple[str, str, str, str]]:
    """Extract (current_state, exit_code_pattern, result_state, comment)
    from next_state_from_check()."""
    text = FSM.read_text()
    fn = re.search(
        r"pub fn next_state_from_check\([^)]*\)\s*->[^{]+\{(.*?)\n\}\n",
        text,
        re.DOTALL,
    )
    if not fn:
        raise RuntimeError("next_state_from_check: body not found")
    body = fn.group(1)
    out = []
    # Pattern: (State, EXPR) => Some(Result),  -- with optional preceding //comments
    rx = re.compile(
        r"((?:\s*//[^\n]*\n)*)"
        r"\s*\(\s*(?P<from>\w+)\s*,\s*(?P<code>[\w_-]+)\s*\)\s*=>\s*"
        r"(?P<result>Some\(\w+\)|None)\s*,",
        re.MULTILINE,
    )
    for m in rx.finditer(body):
        comments = m.group(1)
        # Take the comment block immediately preceding this arm (collapsed onto one line).
        comment = " ".join(
            line.strip().lstrip("/").strip()
            for line in comments.strip().splitlines()
            if line.strip().startswith("//")
        ).strip()
        from_state = m.group("from")
        code_expr = m.group("code")
        result = m.group("result")
        if result == "None":
            result_text = "_(no transition)_"
        else:
            inner = re.match(r"Some\((\w+)\)", result).group(1)
            result_text = f"→ `{inner.upper()}`"
        if code_expr == "0":
            code_label = "0"
        elif code_expr == "_":
            code_label = "any non-zero"
        else:
            code_label = code_expr
        out.append((from_state, code_label, result_text, comment))
    return out


def render_mermaid(pairs: list[tuple[str, str]]) -> str:
    lines = ["```mermaid", "stateDiagram-v2"]
    # State definitions for nicer rendering with shorter labels.
    for s in STATES:
        lines.append(f"    {s} : {s.upper()}")
    # Group "Any → Unreachable" and "Unreachable → Any" as comments to avoid
    # clutter, and emit them as 7 explicit edges each so the rendering library
    # actually draws them. Mermaid v10 does not support fan-in/out shortcuts.
    for frm, to in pairs:
        # Skip degenerate self-loops if any leaked through.
        if frm == to:
            continue
        lines.append(f"    {frm} --> {to}")
    lines.append("```")
    return "\n".join(lines)


def render_matrix(pairs: list[tuple[str, str]]) -> str:
    valid = set(pairs)
    header = "| from \\ to | " + " | ".join(f"`{s.upper()}`" for s in STATES) + " |"
    sep = "|---" * (len(STATES) + 1) + "|"
    rows = [header, sep]
    for frm in STATES:
        cells = [f"`{frm.upper()}`"]
        for to in STATES:
            if frm == to:
                cells.append("—")
            elif (frm, to) in valid:
                cells.append("✓")
            else:
                cells.append("·")
        rows.append("| " + " | ".join(cells) + " |")
    return "\n".join(rows)


def render(pairs: list[tuple[str, str]], decisions: list[tuple[str, str, str, str]]) -> str:
    out = []
    out.append("# FSM reference")
    out.append("")
    out.append("> Auto-generated from `crates/common/src/fsm.rs` and `crates/common/src/types.rs`. Run `scripts/docs/regen.py` to refresh.")
    out.append("")
    out.append("AppControl drives every component through a deterministic finite-state machine. The same logic runs in the backend (authoritative) and in the agent (for local optimisation). A transition is only persisted when `is_valid_transition(from, to)` returns true; invalid transitions are dropped and logged at `warn` level — they never produce a row in `state_transitions`.")
    out.append("")
    out.append("## States")
    out.append("")
    out.append("| State | Meaning |")
    out.append("|---|---|")
    for s in STATES:
        out.append(f"| `{s.upper()}` | {STATE_MEANING[s]} |")
    out.append("")
    out.append("## Transition diagram")
    out.append("")
    out.append("Edges are valid `from → to` pairs accepted by `is_valid_transition`. The `Any → UNREACHABLE` and `UNREACHABLE → Any` rules are expanded explicitly so the diagram is self-contained.")
    out.append("")
    out.append(render_mermaid(pairs))
    out.append("")
    out.append(f"Total accepted transitions: **{len(pairs)}**.")
    out.append("")
    out.append("## Transition matrix")
    out.append("")
    out.append("`✓` = valid; `·` = rejected by `is_valid_transition`; `—` = identity (not a transition).")
    out.append("")
    out.append(render_matrix(pairs))
    out.append("")
    out.append("## Check-result decision table")
    out.append("")
    out.append("`next_state_from_check(current, exit_code)` is the function the agent invokes after every health check. It returns the new state (if any) **before** `is_valid_transition` validates it — the validation pass is what catches any logic error in the decision table.")
    out.append("")
    out.append("| Current | Exit code | Result | Notes |")
    out.append("|---|---|---|---|")
    for from_state, code, result, comment in decisions:
        notes = comment[:200] if comment else ""
        out.append(f"| `{from_state.upper()}` | {code} | {result} | {notes} |")
    out.append("")
    out.append("## Heartbeat timeout")
    out.append("")
    out.append("The backend's `heartbeat_monitor` task runs every 30 seconds. It finds agents whose `last_heartbeat_at` is older than `organizations.heartbeat_timeout_seconds` (default **180s**) and transitions every active component on that agent to `UNREACHABLE` (except `STOPPED` and `STOPPING`, which are preserved). The trigger is recorded as `heartbeat_timeout` in `state_transitions.trigger`, and the previous state is preserved in `state_transitions.details->>'previous_state'` so the original state can be restored when the agent reconnects.")
    out.append("")
    out.append("## Where to look next")
    out.append("")
    out.append("- [Component state in the UI](../USER_GUIDE.md#map-view) — colour coding")
    out.append("- [Sequencer behaviour](../USER_GUIDE.md#operations) — how starts and stops drive transitions")
    out.append("- [Runbooks](../RUNBOOKS.md) — when a component is stuck in a state")
    out.append("- [Troubleshooting](../TROUBLESHOOTING.md#fsm-issues) — diagnose oscillation, stale UNREACHABLE")
    out.append("")
    return "\n".join(out)


def main() -> int:
    pairs = parse_valid_transitions()
    decisions = parse_check_decisions()
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(pairs, decisions))
    print(f"fsm: {len(pairs)} transitions + {len(decisions)} check rules → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
