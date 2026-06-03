#!/usr/bin/env bash
# End-to-end tests: `phx check` on valid and invalid .phx fixtures.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES_DIR="${ROOT}/tests/cli/fixtures"
PHX_BIN="${ROOT}/target/debug/phx"

FIXTURE_OK="${FIXTURES_DIR}/sample.phx"
FIXTURE_ERR="${FIXTURES_DIR}/bad_type.phx"
FIXTURE_RESOLVE_ERR="${FIXTURES_DIR}/missing_main.phx"

NEG_FIXTURES=(
  "bad_type.phx:type mismatch"
  "missing_main.phx:main"
  "use_after_move.phx:moved"
  "mixed_width.phx:invalid"
)

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

for fixture in "${FIXTURE_OK}" "${FIXTURE_ERR}" "${FIXTURE_RESOLVE_ERR}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "missing fixture: ${fixture}" >&2
    exit 1
  fi
done

echo "running: phx check ${FIXTURE_OK} (expect success)"
if ! "${PHX_BIN}" check "${FIXTURE_OK}"; then
  echo "phx check should succeed for ${FIXTURE_OK}" >&2
  exit 1
fi
echo "phx check passed (valid program)"

echo "running: phx check ${FIXTURE_ERR} (expect failure with caret)"
if output="$("${PHX_BIN}" check "${FIXTURE_ERR}" 2>&1)"; then
  echo "phx check should fail for ${FIXTURE_ERR}" >&2
  exit 1
fi
if [[ "${output}" != *"^"* ]] || [[ "${output}" != *"line"* ]]; then
  echo "phx check should show source line and caret for type errors" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx check failed as expected (type error)"

echo "running: phx check ${FIXTURE_RESOLVE_ERR} (expect resolve failure)"
if "${PHX_BIN}" check "${FIXTURE_RESOLVE_ERR}"; then
  echo "phx check should fail for ${FIXTURE_RESOLVE_ERR}" >&2
  exit 1
fi
echo "phx check failed as expected (missing main)"

for entry in "${NEG_FIXTURES[@]}"; do
  name="${entry%%:*}"
  needle="${entry#*:}"
  path="${FIXTURES_DIR}/${name}"
  if [[ ! -f "${path}" ]]; then
    echo "missing fixture: ${path}" >&2
    exit 1
  fi
  echo "running: phx check ${path} (expect failure containing '${needle}')"
  if output="$("${PHX_BIN}" check "${path}" 2>&1)"; then
    echo "phx check should fail for ${path}" >&2
    exit 1
  fi
  if [[ "${output}" != *"${needle}"* ]]; then
    echo "phx check output should mention '${needle}'" >&2
    echo "got: ${output}" >&2
    exit 1
  fi
  if [[ "${name}" == "use_after_move.phx" ]]; then
    if [[ "${output}" != *"note:"* ]]; then
      echo "use-after-move diagnostic should include move-site note" >&2
      echo "got: ${output}" >&2
      exit 1
    fi
  fi
done

echo "all phx check CLI tests passed"
