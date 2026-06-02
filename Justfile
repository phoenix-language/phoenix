default:
    just --list

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

run:
    cargo run -p phx

test:
    cargo test --workspace

test-integration:
    cargo test -p phx-integration-tests

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
