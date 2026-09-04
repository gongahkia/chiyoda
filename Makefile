.DEFAULT_GOAL := verify

.PHONY: fmt lint test python-test evidence build check verify generate-example

fmt:
	cargo fmt --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

python-test:
	PYTHONPATH=python/src python3 -m unittest discover -s python/tests -v

evidence:
	cargo run --locked -p chiyoda -- evidence verify benchmarks/evidence/eindhoven-centraal-platform-2024.json
	cargo run --locked -p chiyoda -- evidence verify benchmarks/evidence/vru-trajectory-2022.json
	cargo run --locked -p chiyoda -- evidence verify benchmarks/evidence/wuppertal-crowdqueue-2018.json
	cargo run --locked -p chiyoda -- evidence verify benchmarks/evidence/eindhoven-centraal-layout-osm-2026.json

build:
	cargo build --workspace --locked

check:
	cargo check --workspace --all-targets --locked

verify: fmt lint test python-test evidence build

generate-example:
	cargo run -p chiyoda -- generate --seed 73 -o examples/generated-interchange.chy
