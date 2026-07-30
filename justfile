default:
    @just --list

build:
    cargo build

release:
    cargo build --release

test-unit:
    cargo test

test: build test-unit
    ./tests/integration.sh

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

deny:
    cargo deny check

verify: fmt-check lint test deny

