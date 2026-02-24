#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

assert_contains() {
  local text="$1"
  local pattern="$2"
  local label="$3"
  if command -v rg >/dev/null 2>&1; then
    printf '%s' "$text" | rg -q --fixed-strings "$pattern" || fail "${label}: missing '${pattern}'"
  else
    printf '%s' "$text" | grep -q -F -- "$pattern" || fail "${label}: missing '${pattern}'"
  fi
}

run_acb() {
  cargo run --quiet --bin acb -- "$@"
}

tmpdir="$(mktemp -d)"
graph_rust="$tmpdir/primary-rust.acb"
graph_multi="$tmpdir/primary-multi.acb"
multilang_repo="$tmpdir/multilang"
mkdir -p "$multilang_repo"

cat > "$multilang_repo/main.rs" <<'RS'
trait Processor {
    fn process(&self) -> i32;
}

struct Worker;

impl Processor for Worker {
    fn process(&self) -> i32 {
        1
    }
}

fn main() {
    let w = Worker;
    let _ = w.process();
}
RS

cat > "$multilang_repo/logic.py" <<'PY'
def run_pipeline(data):
    return [x * 2 for x in data]
PY

cat > "$multilang_repo/ui.ts" <<'TS'
export function renderStatus(ok: boolean): string {
  return ok ? "ready" : "blocked";
}
TS

echo "[1/7] Compile canonical Rust fixture"
compile_rust="$(run_acb -f json compile testdata/rust -o "$graph_rust" --include-tests)"
assert_contains "$compile_rust" '"status": "ok"' "compile rust"
assert_contains "$compile_rust" '"units":' "compile rust"

echo "[2/7] Validate topology, impact, coupling, and test-gap primitives"
symbol_json="$(run_acb -f json query "$graph_rust" symbol --name "Processor")"
assert_contains "$symbol_json" '"query": "symbol"' "symbol query"
assert_contains "$symbol_json" '"count":' "symbol query"

if command -v rg >/dev/null 2>&1; then
  unit_id="$(printf '%s' "$symbol_json" | rg -o '"id":\s*[0-9]+' | head -n1 | rg -o '[0-9]+' || true)"
else
  unit_id="$(printf '%s' "$symbol_json" | grep -o '\"id\": *[0-9]*' | head -n1 | grep -o '[0-9]*' || true)"
fi
[ -n "$unit_id" ] || fail "symbol query returned no unit id"

impact_json="$(run_acb -f json query "$graph_rust" impact --unit-id "$unit_id" --depth 3)"
assert_contains "$impact_json" '"query": "impact"' "impact query"

coupling_json="$(run_acb -f json query "$graph_rust" coupling --limit 10)"
assert_contains "$coupling_json" '"query": "coupling"' "coupling query"

test_gap_json="$(run_acb -f json query "$graph_rust" test-gap --limit 10)"
assert_contains "$test_gap_json" '"query": "test-gap"' "test-gap query"

echo "[3/7] Validate health and spec-drift observability primitives"
health_json="$(run_acb -f json health "$graph_rust")"
assert_contains "$health_json" '"summary":' "health"

hotspots_json="$(run_acb -f json query "$graph_rust" hotspots --limit 10)"
assert_contains "$hotspots_json" '"query": "hotspots"' "hotspots query"

echo "[4/7] Validate multi-language graph coverage"
compile_multi="$(run_acb -f json compile "$multilang_repo" -o "$graph_multi" --include-tests)"
assert_contains "$compile_multi" '"status": "ok"' "compile multi"
assert_contains "$compile_multi" '"languages": 3' "compile multi"

echo "[5/7] Validate storage governance control"
budget_json="$(run_acb -f json budget "$graph_rust" --horizon-years 20 --max-bytes 2147483648)"
assert_contains "$budget_json" '"over_budget":' "budget"

echo "[6/7] Validate strict edge-case behavior (MCP-facing)"
cargo test --quiet --test edge_cases edge_symbol_lookup_invalid_mode
cargo test --quiet --test edge_cases edge_impact_analysis_negative_depth

echo "[7/7] Primary codebase problem checks passed (P09,P10,P11,P12,P13,P14,P15,P16,P22)"
