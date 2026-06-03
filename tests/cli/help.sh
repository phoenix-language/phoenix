#!/usr/bin/env bash
# Smoke test: `phx help` prints usage for check and run.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PHX_BIN="${ROOT}/target/debug/phx"

cd "${ROOT}"

echo "building phx CLI..."
cargo build -q -p phx

if [[ ! -x "${PHX_BIN}" ]]; then
  echo "missing binary: ${PHX_BIN}" >&2
  exit 1
fi

output="$("${PHX_BIN}" help 2>&1)" || true
if [[ "${output}" != *"phx check"* ]] || [[ "${output}" != *"phx run"* ]] || [[ "${output}" != *"phx compile"* ]] || [[ "${output}" != *"module-path"* ]]; then
  echo "phx help should mention check, compile, run, and --module-path" >&2
  echo "got: ${output}" >&2
  exit 1
fi

echo "phx help passed"
