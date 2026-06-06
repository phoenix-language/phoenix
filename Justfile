default:
    just --list

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

# Pass-through to the local `phx` CLI (e.g. `just phx check file.phx`, `just phx run --module-src dir entry.phx`).
phx *args:
    cargo run -p phx -- {{args}}

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
    cargo test -p phx-integration-tests --test cli_e2e --test run_smoke -- --test-threads=1

test-cli: test-lang

dep-check:
    bash tests/ci/check-deps.sh

pre-commit:
    just fmt-check lint doc-check dep-check test-lang

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
