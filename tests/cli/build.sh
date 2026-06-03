#!/usr/bin/env bash
# CLI test: `phx build` and `phx run` with phoenix.toml project layout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"
PROJECT="${ROOT}/tests/cli/fixtures/project"
ENTRY="${PROJECT}/src/main.phx"
BUILD_BIN="${PROJECT}/build/bin/main.phx0"

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

rm -rf "${PROJECT}/build"

echo "running: phx build ${ENTRY}"
"${PHX_BIN}" build "${ENTRY}"

if [[ ! -f "${BUILD_BIN}" ]]; then
  echo "expected linked binary: ${BUILD_BIN}" >&2
  exit 1
fi

if [[ ! -f "${PROJECT}/build/manifest.json" ]]; then
  echo "expected manifest.json" >&2
  exit 1
fi

if [[ ! -f "${PROJECT}/build/pxi/util/math.pxi" ]]; then
  echo "expected util/math.pxi" >&2
  exit 1
fi

echo "running: phx run --no-build ${ENTRY}"
"${PHX_BIN}" run --no-build "${ENTRY}"

echo "phx build/run project tests passed"
