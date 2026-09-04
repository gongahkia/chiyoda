.DEFAULT_GOAL := verify

.PHONY: fmt lint test build check verify generate-example

fmt:
	cargo fmt --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

build:
	cargo build --workspace --locked

check:
	cargo check --workspace --all-targets --locked

verify: fmt lint test build

generate-example:
	cargo run -p chiyoda -- generate --seed 73 -o examples/generated-interchange.chy

