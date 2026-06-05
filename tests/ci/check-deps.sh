#!/usr/bin/env bash
# Fail if any workspace member Cargo.toml declares a crates.io dependency.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
status=0

check_member_toml() {
    local toml="$1"
    local rel="${toml#$ROOT/}"

    while IFS= read -r line; do
        echo "error: external dependency in ${rel}: ${line}" >&2
        status=1
    done < <(
        awk '
            /^\[/ {
                in_deps = ($0 ~ /^\[(dev-|build-)?dependencies\]$/)
                next
            }
            in_deps && /^[[:space:]]*[A-Za-z0-9_.-]+[[:space:]]*=/ {
                line = $0
                if (line ~ /path[[:space:]]*=/) next
                if (line ~ /workspace[[:space:]]*=/) next
                if (line ~ /= *"/ || line ~ /version[[:space:]]*=/) {
                    print line
                }
            }
        ' "$toml"
    )
}

for toml in "$ROOT"/source/*/Cargo.toml "$ROOT/tests/integration/Cargo.toml"; do
    [[ -f "$toml" ]] || continue
    check_member_toml "$toml"
done

if awk '/^\[workspace\.dependencies\]/ { in_ws = 1; next }
         in_ws && /^\[/ { in_ws = 0 }
         in_ws && /version[[:space:]]*=/ { print; found = 1 }
         END { exit !found }' "$ROOT/Cargo.toml" 2>/dev/null; then
    echo "error: [workspace.dependencies] in Cargo.toml must not declare versioned crates.io deps" >&2
    status=1
fi

exit "$status"
