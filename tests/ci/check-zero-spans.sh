#!/usr/bin/env bash
# Fail if production code synthesizes Span::new(0, 0) (caret-at-byte-0 diagnostics).
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

# Test-only paths may use zero spans in #[cfg(test)] helpers.
allowed=(
  source/phx-compiler/src/typeck/bounds.rs
  source/phx-compiler/src/typeck/display.rs
  source/phx-diagnostics/src/lower_error.rs
)

violations=()
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  file="${line%%:*}"
  skip=false
  for a in "${allowed[@]}"; do
    if [[ "$file" == "$a" ]]; then
      skip=true
      break
    fi
  done
  if ! $skip; then
    violations+=("$line")
  fi
done < <(rg 'Span::new\(0, 0\)' source/ -n 2>/dev/null || true)

if ((${#violations[@]} > 0)); then
  echo "error: Span::new(0, 0) found outside test-only allowlist:" >&2
  for v in "${violations[@]}"; do
    echo "  $v" >&2
  done
  echo >&2
  echo "Use the span of the construct being desugared (PHX-063)." >&2
  exit 1
fi

echo "check-zero-spans: ok"
