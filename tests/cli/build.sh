#!/usr/bin/env bash
# CLI test: `phx build` and `phx run` with phoenix.toml project layout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"
PROJECT="${ROOT}/tests/cli/fixtures/project"
BUILD_BIN="${PROJECT}/build/bin/cli_project_test.phx0"

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

rm -rf "${PROJECT}/build"

echo "running: phx build (project default entry)"
"${PHX_BIN}" build --project-root "${PROJECT}"

if [[ ! -f "${BUILD_BIN}" ]]; then
  echo "expected linked binary: ${BUILD_BIN}" >&2
  exit 1
fi

if [[ ! -f "${PROJECT}/build/manifest.json" ]]; then
  echo "expected manifest.json" >&2
  exit 1
fi

if [[ ! -f "${PROJECT}/build/pxi/cli_project_test/util/math.pxi" ]]; then
  echo "expected cli_project_test/util/math.pxi" >&2
  exit 1
fi

echo "running: phx run --no-build --project-root ${PROJECT}"
"${PHX_BIN}" run --no-build --project-root "${PROJECT}" "${PROJECT}/src/main.phx"

MVP_ACCEPTANCE="${ROOT}/tests/cli/fixtures/mvp_acceptance"
MVP_BIN="${MVP_ACCEPTANCE}/build/bin/mvp_acceptance.phx0"

rm -rf "${MVP_ACCEPTANCE}/build"

echo "running: phx build (mvp_acceptance)"
"${PHX_BIN}" build --project-root "${MVP_ACCEPTANCE}"

if [[ ! -f "${MVP_BIN}" ]]; then
  echo "expected linked binary: ${MVP_BIN}" >&2
  exit 1
fi

if [[ ! -f "${MVP_ACCEPTANCE}/build/manifest.json" ]]; then
  echo "expected mvp_acceptance manifest.json" >&2
  exit 1
fi

echo "running: phx run --no-build mvp_acceptance"
"${PHX_BIN}" run --no-build --project-root "${MVP_ACCEPTANCE}" "${MVP_ACCEPTANCE}/src/main.phx"

echo "phx build/run project tests passed"
