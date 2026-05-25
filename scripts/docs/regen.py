#!/usr/bin/env python3
"""Run every `gen_*.py` generator in this directory and report a summary.

This is the entry point used by:
  - `Makefile` (target `docs-reference`)
  - `.github/workflows/docs-pages.yaml` (before `mkdocs build`)
  - Local devs (`python scripts/docs/regen.py`)

Each generator is a self-contained script with a `main()` that returns 0 on
success. Failures here block the docs build — we want a noisy CI failure if
the source-of-truth code has changed in a way the parser can't handle, not a
silently stale reference page.
"""
from __future__ import annotations

import importlib
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))


GENERATORS = [
    "gen_errors",
    "gen_fsm",
    "gen_metrics",
    "gen_configuration",
    "gen_cli",
    "gen_database_schema",
    "gen_api",
    "gen_enums",
    "gen_mcp",
]


def main() -> int:
    failures: list[tuple[str, BaseException]] = []
    t0 = time.monotonic()
    for name in GENERATORS:
        try:
            mod = importlib.import_module(f"docs.{name}")
            rc = mod.main()
            if rc != 0:
                failures.append((name, RuntimeError(f"exit code {rc}")))
        except Exception as e:  # noqa: BLE001 — we want the broad except for CI legibility
            failures.append((name, e))
            print(f"!! {name} failed: {e}", file=sys.stderr)
    dt = time.monotonic() - t0
    if failures:
        print(f"\nFAILED: {len(failures)} of {len(GENERATORS)} generators failed in {dt:.1f}s", file=sys.stderr)
        for name, err in failures:
            print(f"  - {name}: {err}", file=sys.stderr)
        return 1
    print(f"\nOK: {len(GENERATORS)} generators in {dt:.1f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
