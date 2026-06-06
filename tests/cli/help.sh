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
if [[ "${output}" != *"check"* ]] || [[ "${output}" != *"run"* ]] || [[ "${output}" != *"compile"* ]] || [[ "${output}" != *"build"* ]] || [[ "${output}" != *"phoenix.toml"* ]]; then
  echo "phx help should mention check, build, compile, run, and phoenix.toml" >&2
  echo "got: ${output}" >&2
  exit 1
fi

echo "phx help passed"
