.PHONY: build test lint check image

build:
	cargo build --locked

test:
	cargo test --locked

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

check: lint test

image:
	cargo run --locked -- image build
