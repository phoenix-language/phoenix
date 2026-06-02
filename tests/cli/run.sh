#!/usr/bin/env bash
# End-to-end test: `phx run` on a valid sample program.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE_OK="${ROOT}/tests/cli/fixtures/sample.phx"
PHX_BIN="${ROOT}/target/debug/phx"

cd "${ROOT}"

if [[ ! -f "${FIXTURE_OK}" ]]; then
  echo "missing fixture: ${FIXTURE_OK}" >&2
  exit 1
fi

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

echo "running: phx run ${FIXTURE_OK} (expect success)"
if ! "${PHX_BIN}" run "${FIXTURE_OK}"; then
  echo "phx run should succeed for ${FIXTURE_OK}" >&2
  exit 1
fi
echo "phx run passed (valid program)"

echo "all phx run CLI tests passed"
