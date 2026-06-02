#!/usr/bin/env bash
# End-to-end test: `phx run` on valid Phoenix programs.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"

FIXTURES=(
  sample.phx
  control_flow.phx
  continue_in_if.phx
  logical.phx
  match_int.phx
  match_bool.phx
  struct_point.phx
  struct_assign.phx
  enum_match.phx
  struct_method.phx
  cast_width.phx
  mod_bitwise.phx
  array_index.phx
  tuple_lit.phx
  given_struct.phx
  trait_eq.phx
  primitives_float.phx
)

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

for name in "${FIXTURES[@]}"; do
  path="${ROOT}/tests/cli/fixtures/${name}"
  if [[ ! -f "${path}" ]]; then
    echo "missing fixture: ${path}" >&2
    exit 1
  fi
  echo "running: phx run ${path} (expect success)"
  if ! "${PHX_BIN}" run "${path}"; then
    echo "phx run should succeed for ${path}" >&2
    exit 1
  fi
done

echo "all phx run CLI tests passed"
