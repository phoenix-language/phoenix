#!/usr/bin/env bash
# CLI test: `phx build` and `phx run` with phoenix.toml project layout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"
PROJECT="${ROOT}/tests/cli/fixtures/project"
BUILD_BIN="${PROJECT}/build/bin/cli_project_test.phx0"
MATH_LIB="${ROOT}/tests/cli/fixtures/math_lib"
APP_DEP="${ROOT}/tests/cli/fixtures/app_dep"
BAD_DEP_KEY="${ROOT}/tests/cli/fixtures/bad_dep_key"
BIN_MISSING_MAIN="${ROOT}/tests/cli/fixtures/bin_missing_main"

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

if [[ ! -f "${PROJECT}/build/phx0/cli_project_test/util/math.phx0" ]]; then
  echo "expected cli_project_test/util/math.phx0" >&2
  exit 1
fi

echo "running: phx run --no-build --project-root ${PROJECT} (default entry, no file)"
"${PHX_BIN}" run --no-build --project-root "${PROJECT}"

echo "running: phx run --no-build --project-root ${PROJECT} (explicit entry)"
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

MATH_LIB_OUT="${MATH_LIB}/build/lib/math.phx0"

rm -rf "${MATH_LIB}/build"

echo "running: phx build (math_lib standalone lib package)"
"${PHX_BIN}" build --project-root "${MATH_LIB}"

if [[ ! -f "${MATH_LIB_OUT}" ]]; then
  echo "expected linked library: ${MATH_LIB_OUT}" >&2
  exit 1
fi

if [[ ! -f "${MATH_LIB}/build/manifest.json" ]]; then
  echo "expected math_lib manifest.json" >&2
  exit 1
fi

if [[ ! -f "${MATH_LIB}/build/pxi/math.pxi" ]]; then
  echo "expected math_lib build/pxi/math.pxi" >&2
  exit 1
fi

if [[ ! -f "${MATH_LIB}/build/phx0/math.phx0" ]]; then
  echo "expected math_lib build/phx0/math.phx0" >&2
  exit 1
fi

echo "running: phx run --project-root math_lib (expect lib rejection)"
if output="$("${PHX_BIN}" run --project-root "${MATH_LIB}" 2>&1)"; then
  echo "phx run should reject type = lib projects" >&2
  exit 1
fi
if [[ "${output}" != *"project.type = bin"* ]]; then
  echo "lib run rejection should mention project.type = bin" >&2
  echo "got: ${output}" >&2
  exit 1
fi

rm -rf "${APP_DEP}/build"

echo "running: phx build (app_dep with path dependency)"
"${PHX_BIN}" build --project-root "${APP_DEP}"

if [[ ! -f "${APP_DEP}/build/bin/app_dep.phx0" ]]; then
  echo "expected app_dep linked binary" >&2
  exit 1
fi

if [[ ! -f "${APP_DEP}/build/deps/math/lib/math.phx0" ]]; then
  echo "expected path dependency lib at build/deps/math/lib/math.phx0" >&2
  exit 1
fi

echo "running: phx build bad_dep_key (expect dependency key mismatch)"
if output="$("${PHX_BIN}" build --project-root "${BAD_DEP_KEY}" 2>&1)"; then
  echo "phx build should fail for bad dependency key" >&2
  exit 1
fi
if [[ "${output}" != *"dependency key"* ]] && [[ "${output}" != *"invalid phoenix.toml"* ]]; then
  echo "bad dependency key should mention dependency key or invalid phoenix.toml" >&2
  echo "got: ${output}" >&2
  exit 1
fi

echo "running: phx build bin_missing_main (expect missing main.phx)"
if output="$("${PHX_BIN}" build --project-root "${BIN_MISSING_MAIN}" 2>&1)"; then
  echo "phx build should fail when main.phx is missing" >&2
  exit 1
fi
if [[ "${output}" != *"main.phx"* ]] && [[ "${output}" != *"invalid phoenix.toml"* ]]; then
  echo "missing main.phx should mention main.phx or invalid phoenix.toml" >&2
  echo "got: ${output}" >&2
  exit 1
fi

echo "phx build/run project tests passed"
