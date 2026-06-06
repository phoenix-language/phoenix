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
  "invalid_utf8_byte_as_str.phx:invalid cast"
  "given_enum_non_exhaustive.phx:non-exhaustive"
  "match_unreachable_arm.phx:unreachable"
  "trait_impl_incomplete.phx:trait method"
  "deferred_break_value.phx:break"
  "deferred_at_send.phx:@send"
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
if [[ "${output}" != *"^"* ]] || [[ "${output}" != *"-->"* ]]; then
  echo "phx check should show file location and caret for type errors" >&2
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
    if [[ "${output}" != *"= note:"* ]]; then
      echo "use-after-move diagnostic should include move-site note" >&2
      echo "got: ${output}" >&2
      exit 1
    fi
  fi
done

MODULES_DIR="${FIXTURES_DIR}/modules"
MODULE_MAIN="${MODULES_DIR}/main.phx"
MODULE_PRIVATE="${MODULES_DIR}/import_private.phx"
MODULE_CYCLE="${MODULES_DIR}/cycle_a.phx"

for fixture in "${MODULE_MAIN}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "missing fixture: ${fixture}" >&2
    exit 1
  fi
done

echo "running: phx check --module-src ${MODULES_DIR} ${MODULE_MAIN} (expect success)"
if ! "${PHX_BIN}" check --module-src "${MODULES_DIR}" "${MODULE_MAIN}"; then
  echo "phx check should succeed for multi-file modules fixture" >&2
  exit 1
fi
echo "phx check passed (modules import)"

MODULE_LIST="${MODULES_DIR}/main_list.phx"
MODULE_GLOB="${MODULES_DIR}/main_glob.phx"
MODULE_DUP="${MODULES_DIR}/import_dup.phx"

for fixture in "${MODULE_LIST}" "${MODULE_GLOB}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "missing fixture: ${fixture}" >&2
    exit 1
  fi
  echo "running: phx check --module-src ${MODULES_DIR} ${fixture} (expect success)"
  if ! "${PHX_BIN}" check --module-src "${MODULES_DIR}" "${fixture}"; then
    echo "phx check should succeed for ${fixture}" >&2
    exit 1
  fi
done
echo "phx check passed (list and glob imports)"

echo "running: phx check ${MODULE_DUP} (expect duplicate import failure)"
if output="$("${PHX_BIN}" check --module-src "${MODULES_DIR}" "${MODULE_DUP}" 2>&1)"; then
  echo "phx check should fail for duplicate import" >&2
  exit 1
fi
if [[ "${output}" != *"duplicate"* ]] && [[ "${output}" != *"Duplicate"* ]] && [[ "${output}" != *"E1011"* ]]; then
  echo "duplicate import should mention duplicate or E1011" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx check failed as expected (duplicate import)"

echo "running: phx check ${MODULE_PRIVATE} (expect private import failure)"
if output="$("${PHX_BIN}" check --module-src "${MODULES_DIR}" "${MODULE_PRIVATE}" 2>&1)"; then
  echo "phx check should fail for private import" >&2
  exit 1
fi
if [[ "${output}" != *"export"* ]] && [[ "${output}" != *"exported"* ]]; then
  echo "private import should mention export" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx check failed as expected (private import)"

echo "running: phx check ${MODULE_CYCLE} (expect cycle failure)"
if output="$("${PHX_BIN}" check --module-src "${MODULES_DIR}" "${MODULE_CYCLE}" 2>&1)"; then
  echo "phx check should fail for import cycle" >&2
  exit 1
fi
if [[ "${output}" != *"cycle"* ]]; then
  echo "cycle diagnostic should mention cycle" >&2
  echo "got: ${output}" >&2
  exit 1
fi
if [[ "${output}" != *"cycle_a"* ]] || [[ "${output}" != *"cycle_b"* ]]; then
  echo "cycle diagnostic should name modules in the cycle trace" >&2
  echo "got: ${output}" >&2
  exit 1
fi
if [[ "${output}" != *"E1008"* && "${output}" != *"circular module import"* ]]; then
  echo "cycle diagnostic should include E1008 or circular module import" >&2
  echo "got: ${output}" >&2
  exit 1
fi
if [[ "${output}" != *"#import"* && "${output}" != *"import"* ]]; then
  echo "cycle diagnostic should reference an import site" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx check failed as expected (import cycle)"

PROJECT_MAIN="${ROOT}/tests/cli/fixtures/project/src/main.phx"
if [[ ! -f "${PROJECT_MAIN}" ]]; then
  echo "missing fixture: ${PROJECT_MAIN}" >&2
  exit 1
fi
echo "running: phx check ${PROJECT_MAIN} (expect success via phoenix.toml discovery)"
if ! "${PHX_BIN}" check "${PROJECT_MAIN}"; then
  echo "phx check should succeed for project entry without --module-src" >&2
  exit 1
fi
echo "phx check passed (project module_src discovery)"

PROJECT_ROOT="${ROOT}/tests/cli/fixtures/project"
STRAY_FILE="${PROJECT_ROOT}/src/util/math.phx"
echo "running: phx run ${STRAY_FILE} from project (expect standalone file rejection)"
if output="$("${PHX_BIN}" run "${STRAY_FILE}" 2>&1)"; then
  echo "phx run should reject non-entry file inside a project directory" >&2
  exit 1
fi
if [[ "${output}" != *"phoenix.toml"* ]]; then
  echo "stray file run should mention phoenix.toml" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx run rejected stray file in project as expected"

LIB_WITH_MAIN="${ROOT}/tests/cli/fixtures/lib_with_main/src/lib.phx"
echo "running: phx check ${LIB_WITH_MAIN} (expect main forbidden in lib)"
if output="$("${PHX_BIN}" check "${LIB_WITH_MAIN}" 2>&1)"; then
  echo "phx check should fail when lib package defines main" >&2
  exit 1
fi
if [[ "${output}" != *"not allowed in library"* ]] && [[ "${output}" != *"E1014"* ]]; then
  echo "lib main rejection should mention library restriction or E1014" >&2
  echo "got: ${output}" >&2
  exit 1
fi
echo "phx check failed as expected (main forbidden in lib)"

APP_DEP_MAIN="${ROOT}/tests/cli/fixtures/app_dep/src/main.phx"
echo "running: phx check ${APP_DEP_MAIN} without prior build (expect success)"
if ! "${PHX_BIN}" check "${APP_DEP_MAIN}"; then
  echo "phx check should succeed for path-dependency project without build/" >&2
  exit 1
fi
echo "phx check passed (app_dep without build)"

INTERFACE_CHECK_PROJECT="${ROOT}/tests/cli/fixtures/project/src/main.phx"
rm -rf "${ROOT}/tests/cli/fixtures/project/build"
echo "running: phx check --emit-interface-only ${INTERFACE_CHECK_PROJECT}"
"${PHX_BIN}" check --emit-interface-only "${INTERFACE_CHECK_PROJECT}"
if [[ ! -f "${ROOT}/tests/cli/fixtures/project/build/manifest.json" ]]; then
  echo "check --emit-interface-only should write manifest.json" >&2
  exit 1
fi
if [[ -f "${ROOT}/tests/cli/fixtures/project/build/bin/cli_project_test.phx0" ]]; then
  echo "check --emit-interface-only should not produce linked binary" >&2
  exit 1
fi
echo "phx check passed (emit-interface-only)"

echo "all phx check CLI tests passed"
