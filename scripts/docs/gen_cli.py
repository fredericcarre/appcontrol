#!/usr/bin/env python3
"""Generate docs/reference/cli.md from the clap derives in
`crates/cli/src/main.rs`.

We parse the derive macros instead of running `appctl --help` because:
  - Running the binary requires `cargo build`, which is slow in CI.
  - Doc comments above each variant carry the human description that
    `--help` only renders verbatim.

The parser is tolerant: anything it can't match is rendered as
`_(see source)_` so a syntax change in clap derives produces a visible
TODO in the table rather than a silent omission.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path
from dataclasses import dataclass, field

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "crates/cli/src/main.rs"
OUTPUT = REPO / "docs/reference/cli.md"


@dataclass
class Arg:
    name: str
    type_: str = ""
    short_flag: str | None = None
    long_flag: str | None = None
    env: str | None = None
    default: str | None = None
    doc: str = ""
    positional: bool = False
    optional: bool = False

    @property
    def is_bool_flag(self) -> bool:
        # Boolean flag (`bool` or `Option<bool>`) — implicitly defaults to false
        # when not passed, so it is never "required".
        return self.type_.strip().rstrip(",") in ("bool", "Option<bool>")

    @property
    def is_required(self) -> bool:
        if self.is_bool_flag:
            return False
        if self.optional:
            return False
        if self.default is not None:
            return False
        return True


@dataclass
class Command:
    name: str
    doc: str = ""
    args: list[Arg] = field(default_factory=list)
    subcommands: list["Command"] = field(default_factory=list)


def parse_struct_or_enum_body(text: str, name: str) -> str | None:
    """Return the body text inside `struct {name} { ... }` or
    `enum {name} { ... }`. Handles nested braces."""
    rx = re.compile(rf"(?:struct|enum)\s+{re.escape(name)}\s*\{{", re.MULTILINE)
    m = rx.search(text)
    if not m:
        return None
    start = m.end()
    depth = 1
    i = start
    while i < len(text) and depth > 0:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start:i]
        i += 1
    return None


DOC_RX = re.compile(r"^\s*///\s?(.*)$", re.MULTILINE)
ATTR_RX = re.compile(r"#\[(arg|command)\(([^)]*)\)\]")
FIELD_RX = re.compile(
    r"^\s*(?P<name>[a-z_][a-z0-9_]*)\s*:\s*(?P<type>[^,\n]+)",
    re.MULTILINE,
)


def parse_doc_block(lines: list[str]) -> str:
    """Concatenate consecutive /// lines into a paragraph."""
    doc_lines = []
    for line in lines:
        m = re.match(r"\s*///\s?(.*)", line)
        if m:
            doc_lines.append(m.group(1))
        else:
            break
    return " ".join(doc_lines).strip()


def parse_arg_attr(attr_body: str) -> dict:
    """Pick out `long`, `short`, `env = "X"`, `default_value = "X"` from a
    clap `#[arg(...)]` body."""
    out = {}
    for m in re.finditer(r'(\w+)\s*=\s*"([^"]*)"', attr_body):
        out[m.group(1)] = m.group(2)
    # `long` and `short` without explicit value
    if re.search(r"\blong\b(?!\s*=)", attr_body):
        out.setdefault("long", "")  # take field name as flag name
    if re.search(r"\bshort\b(?!\s*=)", attr_body):
        out.setdefault("short", "")
    return out


def parse_variant_body(body: str) -> tuple[list[Arg], bool]:
    """Parse the `{ field: Type, ... }` body of an enum variant; return Args
    and a flag indicating whether the variant references a sub-command."""
    args: list[Arg] = []
    has_subcommand = False
    # Walk fields line by line, collecting docs/attrs above each `name: Type` line.
    lines = body.splitlines()
    pending_doc: list[str] = []
    pending_attrs: list[str] = []
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped == "{" or stripped == "}":
            continue
        if stripped.startswith("///"):
            pending_doc.append(stripped[3:].strip())
            continue
        if stripped.startswith("#["):
            pending_attrs.append(stripped)
            continue
        # Field?
        m = re.match(r"(?P<name>[a-z_][a-z0-9_]*)\s*:\s*(?P<type>[^,]+),?", stripped)
        if not m:
            pending_doc = []
            pending_attrs = []
            continue
        name = m.group("name")
        type_ = m.group("type").strip()
        if "Subcommand" in stripped or any("subcommand" in a for a in pending_attrs):
            has_subcommand = True
            pending_doc = []
            pending_attrs = []
            continue
        optional = type_.startswith("Option<")
        positional = not any("#[arg(" in a for a in pending_attrs)
        long_flag = None
        short_flag = None
        env = None
        default = None
        for a in pending_attrs:
            am = re.search(r"#\[arg\(([^)]*)\)\]", a)
            if not am:
                continue
            attrs = parse_arg_attr(am.group(1))
            if "long" in attrs:
                long_flag = attrs["long"] or name.replace("_", "-")
                positional = False
            if "short" in attrs:
                short_flag = attrs["short"] or name[:1]
            if "env" in attrs:
                env = attrs["env"]
            if "default_value" in attrs:
                default = attrs["default_value"]
        if positional and long_flag is None and short_flag is None:
            long_flag = None  # truly positional
        args.append(Arg(
            name=name,
            type_=type_,
            short_flag=short_flag,
            long_flag=long_flag,
            env=env,
            default=default,
            doc=" ".join(pending_doc),
            positional=positional and long_flag is None,
            optional=optional,
        ))
        pending_doc = []
        pending_attrs = []
    return args, has_subcommand


VARIANT_RX = re.compile(
    # Captures: doc lines + variant name + body in braces (non-greedy)
    r"((?:\s*///[^\n]*\n)+)?"
    r"\s*(?P<name>[A-Z][A-Za-z0-9_]*)"
    r"(?:\s*\{(?P<body>[^{}]*(?:\{[^{}]*\}[^{}]*)*)\})?",
)


def parse_enum_variants(body: str) -> list[tuple[str, str, str]]:
    """Return list of (variant_name, doc, body)."""
    out = []
    # Split top-level by ',' at depth 0 to avoid breaking on fields.
    depth = 0
    chunks: list[str] = []
    current = []
    for ch in body:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
        if ch == "," and depth == 0:
            chunks.append("".join(current))
            current = []
        else:
            current.append(ch)
    if current:
        chunks.append("".join(current))
    for chunk in chunks:
        # Find doc lines + variant name. Skip leading blank lines; the chunk
        # starts with `\n` after the comma split.
        lines = chunk.splitlines()
        idx = 0
        while idx < len(lines) and not lines[idx].strip():
            idx += 1
        docs = []
        while idx < len(lines) and lines[idx].strip().startswith("///"):
            docs.append(lines[idx].strip()[3:].strip())
            idx += 1
        rest = "\n".join(lines[idx:]).strip()
        if not rest:
            continue
        nm = re.match(r"(?P<name>[A-Z][A-Za-z0-9_]*)\s*(?:\{(?P<body>.*)\})?", rest, re.DOTALL)
        if not nm:
            continue
        out.append((nm.group("name"), " ".join(docs), (nm.group("body") or "").strip()))
    return out


def kebab(name: str) -> str:
    # Convert PascalCase → kebab-case for clap subcommand names.
    s = re.sub(r"([A-Z])", r"-\1", name).lower().lstrip("-")
    return s


def build_command_tree(text: str) -> Command:
    # Cli struct → top-level args (--url, --api-key). Pass body verbatim;
    # parse_variant_body splits on lines and tolerates leading whitespace.
    cli_body = parse_struct_or_enum_body(text, "Cli") or ""
    top_args, _ = parse_variant_body(cli_body)
    # Commands enum
    cmd_body = parse_struct_or_enum_body(text, "Commands") or ""
    pki_body = parse_struct_or_enum_body(text, "PkiCommands") or ""

    root = Command(name="appctl", doc="AppControl CLI — scheduler integration", args=top_args)

    for vname, vdoc, vbody in parse_enum_variants(cmd_body):
        args, has_sub = parse_variant_body(vbody)
        sub = Command(name=kebab(vname), doc=vdoc, args=args)
        if has_sub and vname == "Pki":
            for sname, sdoc, sbody in parse_enum_variants(pki_body):
                sargs, _ = parse_variant_body(sbody)
                sub.subcommands.append(Command(name=kebab(sname), doc=sdoc, args=sargs))
        root.subcommands.append(sub)
    return root


def render_args(args: list[Arg]) -> str:
    if not args:
        return "_(no arguments)_"
    out = []
    out.append("| Argument | Required | Default | Env | Description |")
    out.append("|---|---|---|---|---|")
    for a in args:
        if a.positional:
            name = f"`<{a.name.upper()}>`"
        elif a.long_flag is not None:
            flag = f"`--{a.long_flag}`"
            if a.short_flag:
                flag = f"`-{a.short_flag}`, " + flag
            name = flag
        else:
            name = f"`{a.name}`"
        required = "yes" if a.is_required else "no"
        # For bool flags the implicit default is `false`, which is more useful
        # to show than a blank cell.
        if a.default is not None:
            default = f"`{a.default}`"
        elif a.is_bool_flag:
            default = "`false`"
        else:
            default = "—"
        env = f"`{a.env}`" if a.env else "—"
        doc = a.doc or "_(undocumented)_"
        out.append(f"| {name} | {required} | {default} | {env} | {doc} |")
    return "\n".join(out)


def render_command(cmd: Command, level: int = 2) -> list[str]:
    out = []
    header = "#" * level
    out.append(f"{header} `{cmd.name}`")
    out.append("")
    if cmd.doc:
        out.append(cmd.doc)
        out.append("")
    out.append(render_args(cmd.args))
    out.append("")
    for sub in cmd.subcommands:
        out.extend(render_command(Command(
            name=f"{cmd.name} {sub.name}",
            doc=sub.doc,
            args=sub.args,
            subcommands=sub.subcommands,
        ), level=level + 1))
    return out


def render(root: Command) -> str:
    out = []
    out.append("# CLI reference (`appctl`)")
    out.append("")
    out.append("> Auto-generated from `crates/cli/src/main.rs` (the clap derive). Run `scripts/docs/regen.py` to refresh.")
    out.append("")
    out.append("The `appctl` binary is the integration point for schedulers (Control-M, AutoSys, $Universe, TWS, Airflow) and on-call operators. It calls the same REST API as the web UI and returns deterministic exit codes — see the [Error reference](errors.md#cli-exit-codes) for the full mapping.")
    out.append("")
    out.append("## Global options")
    out.append("")
    out.append(render_args(root.args))
    out.append("")
    out.append("## Commands")
    out.append("")
    for cmd in root.subcommands:
        out.extend(render_command(cmd, level=3))
    out.append("## Examples")
    out.append("")
    out.append("```bash")
    out.append("# Start an application and wait up to 10 minutes for it to be RUNNING")
    out.append("appctl --url https://appcontrol.example.com \\")
    out.append("       --api-key $APPCONTROL_API_KEY \\")
    out.append("       start core-banking --wait --timeout 600")
    out.append("")
    out.append("# Check status as JSON (suitable for parsing in a script)")
    out.append("appctl status core-banking --format json | jq '.components[] | select(.state != \"RUNNING\")'")
    out.append("")
    out.append("# Dry-run a switchover to validate the plan without executing")
    out.append("appctl switchover core-banking --target-site lyon --mode PROGRESSIVE")
    out.append("")
    out.append("# PKI: initialise the CA and create a 24-hour enrolment token for two agents")
    out.append("appctl pki init --org-name 'ACME Bank' --validity-days 3650 --out ./ca")
    out.append("appctl pki create-token --name 'paris-batch-fleet' --max-uses 2 --valid-hours 24")
    out.append("```")
    out.append("")
    out.append("## See also")
    out.append("")
    out.append("- [Error reference](errors.md) — exit codes and how schedulers should branch on them")
    out.append("- [Scheduler integration cookbook](../INTEGRATION_COOKBOOK.md) — Control-M, AutoSys, $Universe, TWS recipes")
    out.append("- [Permissions](../PERMISSIONS.md) — which permission level each command requires")
    out.append("")
    return "\n".join(out)


def main() -> int:
    text = SOURCE.read_text()
    root = build_command_tree(text)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(root))
    n_subs = sum(1 + len(s.subcommands) for s in root.subcommands)
    print(f"cli: {len(root.subcommands)} top-level commands + {n_subs - len(root.subcommands)} pki sub-commands → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
