default:
    just --list

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

# Pass-through to the local `phx` CLI (e.g. `just phx check file.phx`, `just phx run --module-src dir entry.phx`).
phx *args:
    cargo run -p phx -- {{args}}

build-std:
    cargo run -p phx -- build --project-root std

# Compile, verify, and execute a Phoenix source file via the CLI.
run file:
    cargo run -p phx -- run {{file}}

compile_source_file target out:
    cargo run -p phx -- compile {{target}} -o {{out}}

test:
    cargo test --workspace

test-integration:
    cargo test -p phx-integration-tests

test-lang:
    cargo test -p phx-integration-tests --test cli_e2e --test run_smoke --test diagnostics

test-cli: test-lang

dep-check:
    bash tests/ci/check-deps.sh
    bash tests/ci/check-compiler-api.sh
    bash tests/ci/check-fixture-gates.sh
    bash tests/ci/check-zero-spans.sh

pre-commit:
    just fmt lint doc-check dep-check test

doc-check:
    cargo doc --workspace --no-deps

lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

doc:
    cargo doc --workspace --no-deps

check: fmt-check lint test build

clean:
    cargo clean

update:
    cargo update

audit:
    cargo audit

# Website (git submodule — see docs/contributing.md and website/Justfile)
website *args:
    just -f website/Justfile {{args}}

# Initialize the website submodule after clone.
website-init:
    git submodule update --init --recursive website

# Show website submodule state vs the pointer recorded in this repo.
website-status:
    @echo "=== website submodule ==="
    @git -C website status -sb
    @git -C website log -1 --oneline
    @echo ""
    @echo "=== parent pointer (HEAD) ==="
    @git ls-tree HEAD website
    @echo ""
    @echo "=== upstream website/main ==="
    @git -C website fetch origin main --quiet 2>/dev/null || true
    @git -C website log -1 --oneline origin/main 2>/dev/null || echo "(could not fetch origin/main)"

# Fast-forward website/ to the latest origin/main.
website-pull:
    just website-init
    git -C website fetch origin
    git -C website checkout main
    git -C website pull --ff-only origin main

# Record the current website submodule SHA in the parent repo.
website-bump message="[infra]: bump website submodule":
    git add website
    @git diff --cached --quiet website && echo "website pointer unchanged — nothing to commit" && exit 0 || git commit -m "{{message}}"

# Pull latest website, then bump the parent pointer if it changed.
website-sync:
    just website-pull
    just website-bump

# Push website/main, then commit the updated submodule pointer here.
website-publish message="[infra]: bump website submodule":
    git -C website push origin main
    just website-bump message="{{message}}"
