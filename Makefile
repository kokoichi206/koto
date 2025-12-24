.DEFAULT_GOAL := help

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

.PHONY: test
test:
	cargo test run

.PHONY: check
check:
	cargo check

.PHONY: fmt
fmt:
	cargo fmt --all

.PHONY: fmt-check
fmt-check:
	cargo fmt --all -- --check

.PHONY: lint
lint:
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: demo
demo:
	cargo run -- --demo --memory

.PHONY: clean
clean:
	cargo clean
