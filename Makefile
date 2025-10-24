# Makefile for Thermos-Rust

.PHONY: help build build-release test clean run docker-build docker-run fmt lint check install

# Default target
help:
	@echo "Thermos-Rust Build System"
	@echo ""
	@echo "Available targets:"
	@echo "  build          - Build debug binary"
	@echo "  build-release  - Build optimized release binary"
	@echo "  test           - Run all tests"
	@echo "  test-verbose   - Run tests with output"
	@echo "  clean          - Remove build artifacts"
	@echo "  run            - Run the application (debug)"
	@echo "  run-release    - Run the application (release)"
	@echo "  docker-build   - Build Docker image"
	@echo "  docker-run     - Run Docker Compose stack"
	@echo "  fmt            - Format code"
	@echo "  lint           - Run clippy linter"
	@echo "  check          - Check code without building"
	@echo "  install        - Install to system"
	@echo "  docs           - Generate documentation"

# Build targets
build:
	cargo build

build-release:
	cargo build --release

# Build for Raspberry Pi (ARM)
build-arm:
	cargo build --release --target armv7-unknown-linux-gnueabihf

build-arm64:
	cargo build --release --target aarch64-unknown-linux-gnu

# Test targets
test:
	cargo test

test-verbose:
	cargo test -- --nocapture

test-coverage:
	cargo tarpaulin --out Html --output-dir coverage

# Run targets
run:
	cargo run

run-release:
	cargo run --release

# Docker targets
docker-build:
	docker build -t thermos-rust:latest .

docker-build-arm:
	docker build -f Dockerfile.armv7 -t thermos-rust:armv7 .

docker-run:
	docker-compose up -d

docker-stop:
	docker-compose down

docker-logs:
	docker-compose logs -f thermostat

# Code quality targets
fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

check:
	cargo check

# Documentation
docs:
	cargo doc --no-deps --open

# Install
install:
	cargo install --path .

# Clean
clean:
	cargo clean
	rm -rf coverage/
	docker-compose down -v

# Setup development environment
setup:
	rustup component add rustfmt clippy
	cargo install cargo-tarpaulin
	@echo "Creating config from example..."
	@if [ ! -f config/config.json ]; then \
		cp config/config.example.json config/config.json; \
		echo "Created config/config.json - please edit with your settings"; \
	fi

# Create systemd service
install-service:
	@echo "Installing systemd service..."
	sudo cp thermos-rust.service /etc/systemd/system/
	sudo systemctl daemon-reload
	sudo systemctl enable thermos-rust
	@echo "Service installed. Start with: sudo systemctl start thermos-rust"

# Development workflow
dev: fmt lint test
	@echo "Development checks complete!"

# Release workflow
release: clean fmt lint test build-release
	@echo "Release build complete!"
	@echo "Binary at: target/release/thermos-rust"

# CI workflow
ci: fmt check lint test
	@echo "CI checks complete!"
