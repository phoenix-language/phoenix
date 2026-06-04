default:
    just --list

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

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
    tests/cli/check.sh
    tests/cli/run.sh
    tests/cli/help.sh

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
