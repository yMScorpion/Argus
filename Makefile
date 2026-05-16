# ARGUS development Makefile.

.PHONY: all fmt fmt-check clippy test bench audit deny ci doc clean build release

all: ci

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace --all-targets

test-prop:
	PROPTEST_CASES=10000 cargo test --workspace --release --test '*property*'

bench:
	cargo bench --workspace

audit:
	cargo audit

deny:
	cargo deny check

doc:
	cargo doc --workspace --no-deps --document-private-items

ci: fmt-check clippy test deny

build:
	cargo build --workspace

release:
	cargo build --workspace --release

clean:
	cargo clean
