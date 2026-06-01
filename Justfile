default:
    just --list

run:
    cargo run

build:
    cargo build --release

test:
    cargo test

clean:
    cargo clean

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

check: fmt-check lint test
    cargo build --all-targets

audit:
    cargo audit

update:
    cargo update

doc:
    cargo doc --no-deps