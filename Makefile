.DEFAULT_GOAL := verify

.PHONY: fmt lint test python-test evidence smoke build check verify generate-example

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

smoke:
	cargo run --locked -p chiyoda -- check examples/experiments/uncalibrated-interchange.chy
	cargo run --locked -p chiyoda -- check examples/eindhoven-centraal-main-entrance-point.chy
	cargo run --locked -p chiyoda -- experiment plan examples/experiments/uncalibrated-interchange.json > /dev/null
	cargo run --locked -p chiyoda -- sensitivity-plan examples/sensitivity/arrival-cadence.json > /dev/null
	cargo run --locked -p chiyoda -- sensitivity-plan examples/sensitivity/exit-capacity-and-trust.json > /dev/null
	cargo run --locked -p chiyoda -- sensitivity-plan examples/sensitivity/urban-reference-speed.json > /dev/null
	cargo run --locked -p chiyoda -- sensitivity-plan examples/sensitivity/crowd-queue-gate-capacity.json > /dev/null

build:
	cargo build --workspace --locked

check:
	cargo check --workspace --all-targets --locked

verify: fmt lint test python-test evidence smoke build

generate-example:
	cargo run -p chiyoda -- generate --seed 73 -o examples/generated-interchange.chy
