.DEFAULT_GOAL := verify

.PHONY: fmt lint test smoke build check verify generate-example

fmt:
	cargo fmt --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

smoke:
	cargo run --locked -p chiyoda -- check examples/demos/grand-interchange-showcase.chy
	smoke_dir=$$(mktemp -d); trap 'rm -rf "$$smoke_dir"' EXIT; \
	cargo run --locked -p chiyoda -- run examples/demos/grand-interchange-showcase.chy -o "$$smoke_dir" --trace-every 20 > /dev/null; \
	cargo run --locked -p chiyoda -- replay "$$smoke_dir/run.json" > /dev/null

build:
	cargo build --workspace --locked

check:
	cargo check --workspace --all-targets --locked

verify: fmt lint test smoke build

generate-example:
	cargo run -p chiyoda -- generate --seed 73 -o examples/generated-interchange.chy
