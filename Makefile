.PHONY: build run release test clean install init discover help

BINARY := crsllm
CONFIG := config/crsllm.toml

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

init: ## Initialize config from template
	@test -f $(CONFIG) || (cp config/crsllm.toml.example $(CONFIG) && echo "✓ Created $(CONFIG) — edit it to add your API keys")
	@test -f $(CONFIG) && echo "Config already exists: $(CONFIG)"

build: ## Build debug binary
	cargo build

release: ## Build optimized release binary
	cargo build --release

run: init ## Build and run (auto-creates config if missing)
	cargo run

dev: init ## Run with hot reload (requires cargo-watch)
	cargo watch -x run

test: ## Run all tests
	cargo test

clean: ## Clean build artifacts
	cargo clean

install: release ## Build release and install to ~/.cargo/bin
	cp target/release/$(BINARY) ~/.cargo/bin/$(BINARY)
	@echo "✓ Installed $(BINARY) to ~/.cargo/bin/"
	@echo "  Run '$(BINARY)' to start the gateway"

discover: init ## Discover models from all providers
	cargo run -- --discover-models

fmt: ## Format code
	cargo fmt

lint: ## Run clippy lints
	cargo clippy -- -D warnings

check: fmt lint test ## Format, lint, and test
