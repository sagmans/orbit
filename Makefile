.PHONY: build dogfood

build:
	cargo build --locked
	cargo install --path . --locked
	cargo run --locked -- image build --no-host-mise-tools

dogfood:
	orbit pi "say hi"
