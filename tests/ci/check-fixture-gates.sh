#!/usr/bin/env bash
# Fail if integration or compiler tests silently skip when fixtures are missing.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

pattern='if ![[:space:]]*[^[:space:]]+\.is_file\(\)[[:space:]]*\{[[:space:]]*return;'

matches=()
while IFS= read -r -d '' file; do
  if grep -E "$pattern" "$file" >/dev/null 2>&1; then
    matches+=("$file")
  fi
done < <(find tests/integration/tests source -path '*/tests/*.rs' -print0 2>/dev/null)

if ((${#matches[@]} > 0)); then
  echo "error: silent fixture-gate pattern found (if !path.is_file() { return; })" >&2
  for f in "${matches[@]}"; do
    echo "  $f" >&2
    grep -nE "$pattern" "$f" >&2 || true
  done
  echo >&2
  echo "Use phx_test::require_cli_project or require_fixture_file instead." >&2
  exit 1
fi

echo "check-fixture-gates: ok"
