#!/usr/bin/env bash
# Generate the Word review version from the Markdown source.
#
# Prerequisite: pandoc (https://pandoc.org)
#   macOS:   brew install pandoc
#   Ubuntu:  sudo apt install pandoc
#   Windows: choco install pandoc  OR  winget install pandoc
#
# Run from the repo root or from docs/:
#   ./docs/build-docx.sh
#
# Output: docs/strategy.docx — open in Word for review and annotations
#         (Track Changes, comments, etc.).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if ! command -v pandoc >/dev/null 2>&1; then
  echo "Error: pandoc is not installed."
  echo "Install: https://pandoc.org/installing.html"
  exit 1
fi

REFERENCE_DOC_ARG=""
if [[ -f "invivoo-reference.docx" ]]; then
  REFERENCE_DOC_ARG="--reference-doc=invivoo-reference.docx"
  echo "Using invivoo-reference.docx as style template."
else
  echo "Note: invivoo-reference.docx is missing — generating it now."
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "import docx" 2>/dev/null || {
      echo "Installing python-docx ..."
      pip install --quiet python-docx 2>/dev/null || pip3 install --quiet python-docx
    }
    python3 build-reference-docx.py && \
      REFERENCE_DOC_ARG="--reference-doc=invivoo-reference.docx"
  else
    echo "      python3 not available — falling back to Pandoc default styles."
  fi
fi

pandoc strategy.md \
  -o strategy.docx \
  --from=markdown+yaml_metadata_block+pipe_tables+definition_lists+fenced_divs \
  --to=docx \
  --toc \
  --toc-depth=2 \
  --number-sections \
  $REFERENCE_DOC_ARG

echo ""
echo "Done — docs/strategy.docx generated."
echo "Open in Word for review with Track Changes and comments."
