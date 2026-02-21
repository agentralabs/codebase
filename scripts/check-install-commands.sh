#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

assert_contains() {
  local pattern="$1"
  shift
  if ! rg -nF "$pattern" "$@" >/dev/null; then
    fail "Missing required install command: ${pattern}"
  fi
}

# Front-facing command requirements
assert_contains "curl -fsSL https://agentralabs.tech/install/codebase | bash" README.md docs/quickstart.md
assert_contains "cargo install agentic-codebase" README.md docs/quickstart.md

# Installer health
bash -n scripts/install.sh
bash scripts/install.sh --dry-run >/dev/null

# Public endpoint/package health
curl -fsSL https://agentralabs.tech/install/codebase >/dev/null
curl -fsSL https://crates.io/api/v1/crates/agentic-codebase >/dev/null

echo "Install command guardrails passed (codebase)."
