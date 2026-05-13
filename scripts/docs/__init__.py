# AppControl documentation generators.
#
# Each `gen_*.py` module in this directory reads source-of-truth code or data
# (Rust enums, migrations, OpenAPI spec, clap derives) and emits a markdown
# reference page under docs/reference/. The MkDocs Material build runs every
# generator in CI via scripts/docs/regen.py before invoking `mkdocs build`,
# so the reference docs cannot drift from the code.
#
# Generators must:
#   - Be idempotent (same input → same output).
#   - Have zero non-stdlib dependencies (CI uses a vanilla Python 3.12).
#   - Print a one-line summary on stdout and exit 0 on success.
#   - Write deterministic markdown (sorted keys, stable iteration).
