#!/usr/bin/env bash
# End-to-end tests: `phx check` on valid and invalid .phx fixtures.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE_OK="${ROOT}/tests/cli/fixtures/sample.phx"
FIXTURE_ERR="${ROOT}/tests/cli/fixtures/bad_type.phx"
FIXTURE_RESOLVE_ERR="${ROOT}/tests/cli/fixtures/missing_main.phx"
PHX_BIN="${ROOT}/target/debug/phx"

cd "${ROOT}"

for fixture in "${FIXTURE_OK}" "${FIXTURE_ERR}" "${FIXTURE_RESOLVE_ERR}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "missing fixture: ${fixture}" >&2
    exit 1
  fi
done

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

echo "running: phx check ${FIXTURE_OK} (expect success)"
if ! "${PHX_BIN}" check "${FIXTURE_OK}"; then
  echo "phx check should succeed for ${FIXTURE_OK}" >&2
  exit 1
fi
echo "phx check passed (valid program)"

echo "running: phx check ${FIXTURE_ERR} (expect failure)"
if "${PHX_BIN}" check "${FIXTURE_ERR}"; then
  echo "phx check should fail for ${FIXTURE_ERR}" >&2
  exit 1
fi
echo "phx check failed as expected (type error)"

echo "running: phx check ${FIXTURE_RESOLVE_ERR} (expect resolve failure)"
if "${PHX_BIN}" check "${FIXTURE_RESOLVE_ERR}"; then
  echo "phx check should fail for ${FIXTURE_RESOLVE_ERR}" >&2
  exit 1
fi
echo "phx check failed as expected (missing main)"

echo "all phx check CLI tests passed"
