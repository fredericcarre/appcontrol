#!/usr/bin/env python3
"""Generate docs/reference/errors.md from crates/backend/src/error.rs.

Parses the ApiError enum and its IntoResponse impl to produce a table of:
    error_type | HTTP Status | When it fires | Example response body

Source of truth: crates/backend/src/error.rs — the (status, error_type, message)
tuple in IntoResponse::into_response is authoritative.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "crates/backend/src/error.rs"
OUTPUT = REPO / "docs/reference/errors.md"

# CLI exit codes — defined in crates/cli/src/main.rs as `const EXIT_*: i32 = N;`.
CLI_SOURCE = REPO / "crates/cli/src/main.rs"


# A row from IntoResponse looks like:
#     ApiError::Forbidden => (
#         StatusCode::FORBIDDEN,
#         "forbidden",
#         "Insufficient permissions".to_string(),
#     ),
# or single-line:
#     ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
#
# We capture (variant, status_const, error_type, message_expr).
ROW_PATTERN = re.compile(
    r"ApiError::(?P<variant>\w+)(?:\([^)]*\))?\s*=>\s*\(\s*"
    r"StatusCode::(?P<status>\w+)\s*,\s*"
    r'"(?P<error_type>[^"]+)"\s*,\s*'
    r"(?P<message>[^,)]+(?:\.to_string\(\)|\.clone\(\))?)",
    re.DOTALL,
)

# Doc comments preceding each variant give us the "when it fires" copy.
VARIANT_DOC_PATTERN = re.compile(
    r'#\[error\("(?P<message>[^"]+)"\)\]\s*\n\s*(?P<variant>\w+)',
)

# CLI exit codes
EXIT_PATTERN = re.compile(
    r"const\s+(EXIT_[A-Z_]+)\s*:\s*i32\s*=\s*(\d+)\s*;",
)


STATUS_TO_NUMBER = {
    "OK": 200,
    "CREATED": 201,
    "NO_CONTENT": 204,
    "BAD_REQUEST": 400,
    "UNAUTHORIZED": 401,
    "FORBIDDEN": 403,
    "NOT_FOUND": 404,
    "CONFLICT": 409,
    "TOO_MANY_REQUESTS": 429,
    "INTERNAL_SERVER_ERROR": 500,
    "SERVICE_UNAVAILABLE": 503,
}


WHEN_IT_FIRES = {
    "Database(sqlx::Error::RowNotFound)": "A `sqlx::query_as!` returns `RowNotFound` for a resource the handler expected to exist.",
    "Database(sqlx::Error::Database(db_err))": "A constraint violation from PostgreSQL (`23505`) or SQLite (`2067`, or message containing `unique constraint`) — typically a duplicate primary key or unique index conflict.",
    "Database(e)": "Any other database error (connection lost, query timeout, syntax error). Logged at `error` level with the full sqlx error; the response message is intentionally generic to avoid leaking schema details.",
    "NotFound": "Explicitly returned by a handler when a path parameter (e.g. `app_id`, `component_id`) does not match an existing row, or via the `OptionExt::ok_or_not_found()` shorthand.",
    "Forbidden": "Effective permission check (`core::permissions::effective_permission`) returns less than the required level for the action, or workspace site access (`can_access_site`) denies the user.",
    "Unauthorized": "No valid JWT, expired JWT, or no `Authorization: Bearer <key>` API key header on a route that requires one. The auth middleware short-circuits before the handler runs.",
    "Conflict(msg)": "Business-logic conflict detected by the handler — for example, deleting a component that still has dependencies, or starting an application that is already in STARTING.",
    "Validation(msg)": "Request body failed `validate_length` / `validate_optional_length`, or a handler-specific check (date range invalid, slug malformed, enum value not in allowed set).",
    "Internal(msg)": "An unexpected error that the handler explicitly converted to `ApiError::Internal(...)`. Logged at `error` level; the message string is hidden from the client.",
    "ServiceUnavailable": "A dependency required to satisfy the request is down — typically the gateway is unreachable when the handler tries to dispatch a command to an agent.",
}


def parse_errors() -> list[dict]:
    text = SOURCE.read_text()
    seen = set()
    rows = []
    for m in ROW_PATTERN.finditer(text):
        # The IntoResponse match is the source of truth — but the same variant
        # can appear multiple times (e.g. Database(_) is matched three ways).
        # We key on (variant, error_type) and keep the first occurrence so that
        # the more-specific `RowNotFound` / `Database(db_err)` rows come first.
        variant = m.group("variant")
        # Reconstruct a discriminator for the documented "fires when" lookup
        # by scanning back to the enum pattern (which includes the inner
        # match like `sqlx::Error::RowNotFound`).
        start = m.start()
        # Walk back to the line beginning so we can capture the full pattern.
        line_start = text.rfind("\n", 0, start) + 1
        full_pattern = text[line_start:start + len("ApiError::") + len(variant)]
        # Find the inner pattern, if any: ApiError::Database(sqlx::Error::RowNotFound)
        # Look 80 chars ahead for the opening => to grab everything in between.
        head = text[line_start:line_start + 200].split("=>")[0]
        head = head.strip()
        # Remove leading "ApiError::"
        if head.startswith("ApiError::"):
            head = head[len("ApiError::"):]
        # head is now e.g. "Database(sqlx::Error::RowNotFound)" or "Forbidden"
        key = (variant, head)
        if key in seen:
            continue
        seen.add(key)
        rows.append({
            "variant": variant,
            "pattern": head,
            "status": m.group("status"),
            "status_code": STATUS_TO_NUMBER.get(m.group("status"), "?"),
            "error_type": m.group("error_type"),
            "fires_when": WHEN_IT_FIRES.get(head, "_(undocumented — see source)_"),
        })
    return rows


def parse_exit_codes() -> list[tuple[str, int]]:
    text = CLI_SOURCE.read_text()
    return [(m.group(1), int(m.group(2))) for m in EXIT_PATTERN.finditer(text)]


def render(rows: list[dict], exits: list[tuple[str, int]]) -> str:
    out = []
    out.append("# Error reference")
    out.append("")
    out.append("> Auto-generated from `crates/backend/src/error.rs` and `crates/cli/src/main.rs`. Do not edit by hand — run `scripts/docs/regen.py` after changing the source.")
    out.append("")
    out.append("## HTTP error responses")
    out.append("")
    out.append("Every backend handler returns errors through the `ApiError` enum, which serialises to a JSON body with two fields:")
    out.append("")
    out.append("```json")
    out.append('{ "error": "<error_type>", "message": "<human-readable message>" }')
    out.append("```")
    out.append("")
    out.append("The `error` field is stable and safe for programmatic dispatch (alerting rules, retry logic). The `message` field is human-readable and may change between versions.")
    out.append("")
    out.append("| HTTP | `error` field | Variant | When it fires |")
    out.append("|---:|---|---|---|")
    for r in rows:
        out.append(f"| {r['status_code']} | `{r['error_type']}` | `ApiError::{r['pattern']}` | {r['fires_when']} |")
    out.append("")
    out.append("### Retry guidance")
    out.append("")
    out.append("| `error` field | Safe to retry? | Recommended back-off |")
    out.append("|---|---|---|")
    out.append("| `unauthorized` | No — refresh the token first | n/a |")
    out.append("| `forbidden` | No — escalate to an operator | n/a |")
    out.append("| `not_found` | No | n/a |")
    out.append("| `validation_error` | No — fix the request | n/a |")
    out.append("| `conflict` | Depends on the body — usually no | n/a |")
    out.append("| `database_error` | Yes, with jitter | exponential, 1s → 30s, cap at 5 retries |")
    out.append("| `service_unavailable` | Yes | exponential, 5s → 60s, cap at 10 retries |")
    out.append("| `internal_error` | Sparingly | linear, 30s, cap at 3 retries |")
    out.append("")
    out.append("## CLI exit codes")
    out.append("")
    out.append("The `appctl` CLI returns the following exit codes — schedulers can dispatch on these without parsing stdout. The mapping below is read from `crates/cli/src/main.rs`.")
    out.append("")
    out.append("| Code | Constant | Meaning |")
    out.append("|---:|---|---|")
    meanings = {
        "EXIT_SUCCESS": "The action completed (and, when `--wait` was passed, the target state was reached).",
        "EXIT_FAILURE": "The action failed for a reason that is not covered by a more specific code.",
        "EXIT_TIMEOUT": "`--wait` was passed and the timeout elapsed before the target state was reached.",
        "EXIT_AUTH_ERROR": "Authentication failed — missing or invalid `--api-key` / `APPCONTROL_API_KEY`.",
        "EXIT_NOT_FOUND": "The application (or component) named on the command line does not exist.",
        "EXIT_PERMISSION_DENIED": "The API key is valid but lacks the required permission level on the target application.",
    }
    for name, code in sorted(exits, key=lambda x: x[1]):
        out.append(f"| {code} | `{name}` | {meanings.get(name, '_(undocumented)_')} |")
    out.append("")
    out.append("### Scheduler integration example")
    out.append("")
    out.append("```bash")
    out.append("appctl start core-banking --wait --timeout 600")
    out.append("case $? in")
    out.append("  0) echo \"Started; downstream jobs may run.\";;")
    out.append("  2) echo \"Timed out; trigger on-call.\"; exit 1;;")
    out.append("  3|5) echo \"Auth or permission issue; rotate the API key.\"; exit 1;;")
    out.append("  *) echo \"Failed; check the action_log for the trigger.\"; exit 1;;")
    out.append("esac")
    out.append("```")
    out.append("")
    return "\n".join(out)


def main() -> int:
    rows = parse_errors()
    exits = parse_exit_codes()
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(rows, exits))
    print(f"errors: {len(rows)} HTTP variants + {len(exits)} exit codes → {OUTPUT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
