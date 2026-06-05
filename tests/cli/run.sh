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
  match_ident.phx
  match_bool.phx
  struct_point.phx
  struct_assign.phx
  enum_match.phx
  enum_match_struct.phx
  struct_method.phx
  cast_width.phx
  compare_unary.phx
  deep_logical_chain.phx
  deep_logical_or_chain.phx
  mod_bitwise.phx
  array_index.phx
  tuple_lit.phx
  given_struct.phx
  trait_eq.phx
  primitives_float.phx
  primitives_width.phx
  primitives_i128.phx
  byte_string.phx
  string_literal.phx
  byte_string_as_str.phx
  ref_local.phx
  deref_ptr.phx
  slice_from_array.phx
  factorial.phx
  given_enum_single_variant.phx
  generic_fn.phx
  generic_struct.phx
  generic_enum.phx
  generic_infer.phx
  generic_impl_method.phx
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

MODULES_DIR="${ROOT}/tests/cli/fixtures/modules"
MODULE_MAIN="${MODULES_DIR}/main.phx"
if [[ ! -f "${MODULE_MAIN}" ]]; then
  echo "missing fixture: ${MODULE_MAIN}" >&2
  exit 1
fi
echo "running: phx run --module-src ${MODULES_DIR} ${MODULE_MAIN} (expect success)"
if ! "${PHX_BIN}" run --module-src "${MODULES_DIR}" "${MODULE_MAIN}"; then
  echo "phx run should succeed for multi-file modules fixture" >&2
  exit 1
fi

echo "all phx run CLI tests passed"
