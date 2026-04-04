#!/usr/bin/env bash
# =============================================================================
# SQLite Safety Lint — prevents common SQLite compatibility regressions
# =============================================================================
# Checks:
# 1. INSERT without `id` column into tables that have `id TEXT PRIMARY KEY`
# 2. Raw Uuid in query_as/query_scalar return types (should be DbUuid)
# 3. Raw .bind(uuid_var) without bind_id() wrapper (heuristic, warnings only)
#
# Only flags issues in SHARED code and SQLite-specific code.
# PostgreSQL-only code (#[cfg(feature = "postgres")]) is excluded.
# =============================================================================
set -euo pipefail

REPO_DIR="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
BACKEND_SRC="$REPO_DIR/crates/backend/src/repository"
MIGRATIONS_SQLITE="$REPO_DIR/migrations/sqlite"

ERRORS=0
WARNINGS=0

red()    { printf '\033[1;31m%s\033[0m\n' "$*"; }
yellow() { printf '\033[1;33m%s\033[0m\n' "$*"; }
green()  { printf '\033[1;32m%s\033[0m\n' "$*"; }

# =============================================================================
# HELPER: Determine if a line is inside a #[cfg(feature = "postgres")] block
# =============================================================================
# Strategy: for each file, build a list of PG-only line ranges.
# PG range = from #[cfg(feature = "postgres")] to the next #[cfg(all(feature = "sqlite"
# or to the next top-level non-indented function/pub/struct outside any impl.
#
# For simplicity, we pair consecutive PG/SQLite markers:
#   PG marker at line X, SQLite marker at line Y → PG range is [X, Y-1]
# =============================================================================

# Build PG-only ranges for a file. Output: "start:end" pairs, one per line.
pg_ranges_for_file() {
    local file="$1"
    local pg_lines sqlite_lines
    pg_lines=$(grep -n '#\[cfg(feature = "postgres")\]' "$file" | cut -d: -f1 || true)
    sqlite_lines=$(grep -n '#\[cfg(all(feature = "sqlite"' "$file" | cut -d: -f1 || true)

    # If no cfg markers, all code is shared
    if [ -z "$pg_lines" ]; then
        return
    fi

    # For each PG marker, find the nearest following SQLite marker
    for pg_start in $pg_lines; do
        local best_end=""
        for sq_start in $sqlite_lines; do
            if [ "$sq_start" -gt "$pg_start" ]; then
                best_end=$((sq_start - 1))
                break
            fi
        done
        if [ -z "$best_end" ]; then
            # PG block extends to end of file
            best_end=$(wc -l < "$file")
        fi
        echo "${pg_start}:${best_end}"
    done
}

# Check if a line number falls in any PG-only range for a file.
# Usage: is_pg_only <file> <lineno>
# Returns 0 (true) if line is in PG-only code, 1 otherwise.
declare -A PG_RANGES_CACHE

is_pg_only() {
    local file="$1"
    local lineno="$2"

    # Cache ranges per file
    if [ -z "${PG_RANGES_CACHE[$file]+x}" ]; then
        PG_RANGES_CACHE[$file]=$(pg_ranges_for_file "$file")
    fi

    local ranges="${PG_RANGES_CACHE[$file]}"
    if [ -z "$ranges" ]; then
        return 1  # No PG ranges → shared code
    fi

    while IFS=: read -r start end; do
        if [ "$lineno" -ge "$start" ] && [ "$lineno" -le "$end" ]; then
            return 0  # In PG-only range
        fi
    done <<< "$ranges"

    return 1  # Not in any PG range
}

# =============================================================================
# CHECK 1: INSERT statements missing `id` column
# =============================================================================
echo "=== CHECK 1: INSERT statements missing 'id' column ==="

# Build list of tables with id TEXT PRIMARY KEY from SQLite migrations.
# CREATE TABLE and "id TEXT PRIMARY KEY" are on separate lines, so we
# look for CREATE TABLE lines and check if the next non-empty line has
# "id TEXT PRIMARY KEY".
TABLES_WITH_ID=()
if [ -d "$MIGRATIONS_SQLITE" ]; then
    for sqlfile in "$MIGRATIONS_SQLITE"/*.sql; do
        [ -f "$sqlfile" ] || continue
        prev_table=""
        while IFS= read -r line; do
            # Check for CREATE TABLE
            if table_name=$(echo "$line" | grep -oP 'CREATE TABLE\s+\K[a-z_]+'); then
                prev_table="$table_name"
            elif [ -n "$prev_table" ] && echo "$line" | grep -qP '^\s+id TEXT PRIMARY KEY'; then
                TABLES_WITH_ID+=("$prev_table")
                prev_table=""
            elif [ -n "$prev_table" ] && echo "$line" | grep -qE '^\s*[a-z]'; then
                # Next column definition but not id TEXT PRIMARY KEY → reset
                prev_table=""
            fi
        done < "$sqlfile"
    done
fi

if [ ${#TABLES_WITH_ID[@]} -eq 0 ]; then
    yellow "  WARN: No tables with 'id TEXT PRIMARY KEY' found in SQLite migrations"
else
    echo "  Found ${#TABLES_WITH_ID[@]} tables with 'id TEXT PRIMARY KEY'"

    for table in "${TABLES_WITH_ID[@]}"; do
        while IFS=: read -r file lineno content; do
            # Skip PG-only code (PG has DEFAULT gen_random_uuid())
            if is_pg_only "$file" "$lineno"; then
                continue
            fi

            # Extract column list from INSERT INTO table (col1, col2, ...)
            col_list=$(echo "$content" | sed -n "s/.*INSERT INTO[[:space:]]*${table}[[:space:]]*(\([^)]*\)).*/\1/p")
            if [ -n "$col_list" ]; then
                if ! echo "$col_list" | grep -qw 'id'; then
                    red "  ERROR: $file:$lineno — INSERT INTO $table missing 'id' column"
                    echo "         Columns: ($col_list)"
                    ERRORS=$((ERRORS + 1))
                fi
            fi
        done < <(grep -rn "INSERT INTO ${table}[[:space:]]*(" "$BACKEND_SRC" 2>/dev/null || true)
    done
fi

# =============================================================================
# CHECK 2: Raw Uuid in query_as/query_scalar return types (should be DbUuid)
# =============================================================================
echo ""
echo "=== CHECK 2: Raw Uuid in query_as/query_scalar return types ==="

# query_as::<_, (... Uuid ...)>
while IFS=: read -r file lineno content; do
    if is_pg_only "$file" "$lineno"; then continue; fi
    red "  ERROR: $file:$lineno — query_as with raw Uuid (use DbUuid)"
    echo "         $(echo "$content" | xargs)"
    ERRORS=$((ERRORS + 1))
done < <(grep -rn 'query_as::<_,.*[( ,]Uuid[) ,]' "$BACKEND_SRC" 2>/dev/null | grep -v 'DbUuid' | grep -v '^\s*//' || true)

# query_scalar::<_, Uuid>
while IFS=: read -r file lineno content; do
    if is_pg_only "$file" "$lineno"; then continue; fi
    red "  ERROR: $file:$lineno — query_scalar with raw Uuid (use DbUuid)"
    echo "         $(echo "$content" | xargs)"
    ERRORS=$((ERRORS + 1))
done < <(grep -rn 'query_scalar::<_,[[:space:]]*Uuid>' "$BACKEND_SRC" 2>/dev/null | grep -v 'DbUuid' | grep -v '^\s*//' || true)

# =============================================================================
# CHECK 3: Raw UUID binds without bind_id() — heuristic, WARNINGS only
# =============================================================================
echo ""
echo "=== CHECK 3: Raw UUID binds without bind_id() (heuristic) ==="

RAW_BIND_COUNT=0
while IFS=: read -r file lineno content; do
    if is_pg_only "$file" "$lineno"; then continue; fi

    # Skip if already uses bind_id, DbUuid, bind_opt_id, .to_string(), or literals
    if echo "$content" | grep -qE '(bind_id|DbUuid|bind_opt_id|\.to_string\(\)|"[^"]*"|'\''[^'\'']*'\''|true|false|None|serde_json)'; then
        continue
    fi

    # Extract bind argument
    bind_arg=$(echo "$content" | sed -n 's/.*\.bind(\([^)]*\)).*/\1/p')
    # Only flag if argument name contains 'id' (likely a UUID)
    if echo "$bind_arg" | grep -qiE '(_id$|^id$|_ids$|uuid)'; then
        yellow "  WARN: $file:$lineno — possible raw UUID bind: .bind($bind_arg)"
        RAW_BIND_COUNT=$((RAW_BIND_COUNT + 1))
        WARNINGS=$((WARNINGS + 1))
    fi
done < <(grep -rn '\.bind(' "$BACKEND_SRC" 2>/dev/null | grep -v '^\s*//' || true)

if [ "$RAW_BIND_COUNT" -eq 0 ]; then
    echo "  No suspicious raw UUID binds found"
fi

# =============================================================================
# SUMMARY
# =============================================================================
echo ""
echo "========================================="
if [ "$ERRORS" -gt 0 ]; then
    red "FAILED: $ERRORS error(s), $WARNINGS warning(s)"
    exit 1
elif [ "$WARNINGS" -gt 0 ]; then
    yellow "PASSED with $WARNINGS warning(s) (non-blocking)"
    exit 0
else
    green "ALL CHECKS PASSED"
    exit 0
fi
