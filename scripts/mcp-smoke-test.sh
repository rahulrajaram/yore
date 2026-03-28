#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_dir="$repo_root/tests/fixtures/mcp-smoke"

if [ ! -d "$fixture_dir/docs" ]; then
  echo "Fixture directory missing: $fixture_dir/docs" >&2
  exit 1
fi

run_yore() {
  if [ -n "${YORE_BIN:-}" ]; then
    "$YORE_BIN" "$@"
  else
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -- "$@"
  fi
}

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

cp -R "$fixture_dir/docs" "$tmpdir/docs"

(
  cd "$tmpdir"
  run_yore build docs --output .yore-smoke >/dev/null

  search_json="$(run_yore mcp search-context authentication \
    --max-results 3 \
    --max-tokens 120 \
    --max-bytes 600 \
    --index .yore-smoke)"

  printf '%s\n' "$search_json"

  printf '%s\n' "$search_json" | grep -q '"schema_version": 1'
  printf '%s\n' "$search_json" | grep -q '"tool": "search_context"'
  printf '%s\n' "$search_json" | grep -q '"selection_mode": "query"'
  printf '%s\n' "$search_json" | grep -q '"source": {'
  printf '%s\n' "$search_json" | grep -q '"preview": "'
  printf '%s\n' "$search_json" | grep -q '"trace_id": "trc_'
  printf '%s\n' "$search_json" | grep -q '"index_fingerprint": "idx_'
  printf '%s\n' "$search_json" | grep -q '"strategy": "lexical"'

  handle="$(printf '%s\n' "$search_json" | sed -n 's/.*"handle": "\(ctx_[^"]*\)".*/\1/p' | head -n1)"
  if [ -z "$handle" ]; then
    echo "Failed to extract handle from search-context output." >&2
    exit 1
  fi

  fetch_json="$(run_yore mcp fetch-context "$handle" \
    --max-tokens 40 \
    --max-bytes 220 \
    --index .yore-smoke)"

  printf '\n%s\n' "$fetch_json"

  printf '%s\n' "$fetch_json" | grep -q '"tool": "fetch_context"'
  printf '%s\n' "$fetch_json" | grep -q "\"handle\": \"$handle\""
  printf '%s\n' "$fetch_json" | grep -q '"truncated": true'
  printf '%s\n' "$fetch_json" | grep -q '\[truncated\]'
  printf '%s\n' "$fetch_json" | grep -q '"strategy": "artifact_fetch"'
)
