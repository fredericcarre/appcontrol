# AppControl — convenience targets. Cargo, npm, and mkdocs all stay the
# canonical entry points; this Makefile only wraps multi-step flows so a
# new contributor can run them without memorising the recipe.

.PHONY: help docs docs-reference docs-serve docs-build docs-clean

help:
	@echo "Documentation targets:"
	@echo "  make docs           Regenerate reference docs + build the static site (site/)"
	@echo "  make docs-reference Regenerate docs/reference/*.md from code only"
	@echo "  make docs-serve     Live-preview the site at http://127.0.0.1:8000"
	@echo "  make docs-build     Build the static site (no regen — fast)"
	@echo "  make docs-clean     Remove generated reference docs and the built site"

# Regenerate every reference page from the source-of-truth Rust / SQL / OpenAPI.
# This is what CI runs before `mkdocs build`. Reproducible with no extra deps
# beyond Python 3 stdlib.
docs-reference:
	python3 scripts/docs/regen.py

# Full docs pipeline: regenerate references → stage root .md → build site/.
# The root-doc staging mirrors what .github/workflows/docs-pages.yaml does so
# the local preview matches the published site byte-for-byte (modulo the git
# revision timestamps).
docs: docs-reference
	@for f in SECURITY_ARCHITECTURE.md CHANGELOG.md RELEASE.md; do \
		if [ -f "$$f" ]; then cp "$$f" "docs/$$f"; fi; \
	done
	mkdocs build --clean

docs-build:
	mkdocs build --clean

docs-serve: docs-reference
	@for f in SECURITY_ARCHITECTURE.md CHANGELOG.md RELEASE.md; do \
		if [ -f "$$f" ]; then cp "$$f" "docs/$$f"; fi; \
	done
	mkdocs serve

docs-clean:
	rm -rf site
	rm -rf docs/reference
	rm -f docs/SECURITY_ARCHITECTURE.md docs/CHANGELOG.md docs/RELEASE.md
