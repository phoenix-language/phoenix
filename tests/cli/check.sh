#!/usr/bin/env bash
# End-to-end test: `phx check` on a real .phx source file.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="${ROOT}/tests/cli/fixtures/sample.phx"
PHX_BIN="${ROOT}/target/debug/phx"

cd "${ROOT}"

if [[ ! -f "${FIXTURE}" ]]; then
  echo "missing fixture: ${FIXTURE}" >&2
  exit 1
fi

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

echo "running: phx check ${FIXTURE}"
if ! "${PHX_BIN}" check "${FIXTURE}"; then
  echo "phx check failed for ${FIXTURE}" >&2
  exit 1
fi

echo "phx check passed"
