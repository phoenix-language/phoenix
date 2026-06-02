#!/usr/bin/env bash
# CLI test: `phx compile -o` writes verifiable bytecode.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"
FIXTURE="${ROOT}/tests/cli/fixtures/sample.phx"
OUT="${ROOT}/target/phx-cli-sample.phx0"

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

rm -f "${OUT}"
echo "running: phx compile ${FIXTURE} -o ${OUT}"
"${PHX_BIN}" compile "${FIXTURE}" -o "${OUT}"

if [[ ! -f "${OUT}" ]]; then
  echo "expected output file: ${OUT}" >&2
  exit 1
fi

if [[ ! -s "${OUT}" ]]; then
  echo "output file is empty" >&2
  exit 1
fi

echo "compile CLI test passed"
