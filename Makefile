.PHONY: build optimize test test-verbose clean fmt fmt-check lint

# Default target
all: build

# Build all contracts (release profile, default for stellar contract build)
build:
	stellar contract build

# Build optimized contracts for deployment
optimize:
	stellar contract build --optimize
	@echo "Optimized contracts are in target/wasm32v1-none/release/*.wasm"

# Run all tests
test:
	cargo test

# Run tests with output
test-verbose:
	cargo test -- --nocapture

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt -- --check

# Run clippy linter
lint:
	cargo clippy --workspace --all-targets --all-features -- \
		-D warnings \
		-A unused-imports \
		-A unused-variables \
		-A clippy::too-many-arguments \
		-A clippy::unnecessary-cast
