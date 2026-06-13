#!/usr/bin/env bash
# Fail if phx-compiler lib.rs re-exports internal pipeline graphs at the crate root.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LIB="$ROOT/source/phx-compiler/src/lib.rs"

if rg -q 'pub use ir::|pub use typeck::TypedProgram|pub use resolver::ResolvedProgram|apply_mono_worklist' "$LIB"; then
    echo "error: phx-compiler internal types must live under phx_compiler::unstable, not lib.rs re-exports" >&2
    exit 1
fi
