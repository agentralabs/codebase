#!/bin/bash
# Pre-publish checks and dry-run for crates.io
set -euo pipefail

DO_PUBLISH=false
if [ "${1:-}" = "--publish" ]; then
  DO_PUBLISH=true
fi

VERSION="$(grep -m1 '^version\\s*=\\s*"' Cargo.toml | sed -E 's/.*"([^"]+)".*/\\1/')"
NOTE_DIR="release-notes"
NOTE_FILE="${NOTE_DIR}/v${VERSION}.md"

ensure_release_note() {
  mkdir -p "${NOTE_DIR}"
  if [ ! -f "${NOTE_FILE}" ]; then
    cat > "${NOTE_FILE}" <<EOF
## TEMPLATE_DRAFT: REPLACE BEFORE PUBLISH

## Executive Summary

AgenticCodebase v${VERSION} advances production-readiness with measurable delivery outcomes and explicit operational expectations for integrators.

## Business Impact

This release reduces onboarding friction, improves release-risk visibility, and tightens the execution path from local validation to shared deployment environments.

## Rollout Guidance

Roll out through staging first, verify MCP/server wiring and compatibility checks, then promote to broader environments with release telemetry enabled.

## Source Links

- https://github.com/agentralabs/codebase/compare/v${VERSION}...HEAD
EOF
    echo "Release note template created at ${NOTE_FILE}."
    echo "Publish gate blocked until you replace template text with final business notes."
    exit 1
  fi

  python3 - <<'PY' "${NOTE_FILE}"
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
required = [
    "## Executive Summary",
    "## Business Impact",
    "## Rollout Guidance",
    "## Source Links",
]
for heading in required:
    if heading not in text:
        print(f"Missing required heading: {heading}")
        sys.exit(1)

if "template_draft" in text.lower():
    print("Template marker still present in release notes.")
    sys.exit(1)

if "as an ai" in text.lower():
    print("Release notes contain forbidden phrasing: as an ai")
    sys.exit(1)

paragraphs = []
for block in re.split(r"\n\s*\n", text):
    b = block.strip()
    if not b or b.startswith("##") or b.startswith("- "):
        continue
    paragraphs.append(b)

if len(paragraphs) < 3:
    print("Release note must contain at least 3 narrative paragraphs.")
    sys.exit(1)

for idx, p in enumerate(paragraphs[:3], start=1):
    if len(p) < 120:
        print(f"Paragraph {idx} is too short ({len(p)} chars).")
        sys.exit(1)
PY
}

ensure_release_note

echo "Running pre-publish checks..."
echo ""

echo "1. Running tests..."
cargo test --workspace
echo ""

echo "2. Checking formatting..."
cargo fmt --all -- --check
echo ""

echo "3. Running clippy..."
cargo clippy --workspace -- -D warnings
echo ""

echo "4. Dry-run publish (single crate ships both acb and agentic-codebase-mcp binaries)..."
cargo publish --dry-run
echo ""

echo "All checks passed!"
echo ""
if [ "${DO_PUBLISH}" = true ]; then
  echo "Publishing crate..."
  cargo publish

  if ! command -v gh >/dev/null 2>&1; then
    echo "Error: gh CLI is required to create GitHub release notes."
    exit 1
  fi

  echo "Creating GitHub release..."
  gh release create "v${VERSION}" \
    --title "AgenticCodebase v${VERSION}" \
    --notes-file "${NOTE_FILE}" \
    --target "$(git rev-parse HEAD)"
  echo "Publish + release complete."
else
  echo "To publish:"
  echo "  ./scripts/publish.sh --publish"
  echo ""
  echo "Note: agentic-codebase publishes both binaries:"
  echo "  - acb"
  echo "  - agentic-codebase-mcp"
fi
