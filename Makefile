# Makefile for Thermos-Rust

.PHONY: help build build-release test clean run docker-build docker-run fmt lint check install install-hooks

# Detect operating system
UNAME_S := $(shell uname -s)

# Default target
help:
	@echo "Thermos-Rust Build System"
	@echo ""
	@echo "Detected OS: $(UNAME_S)"
ifeq ($(UNAME_S),Linux)
	@echo "Platform: Linux (full hardware support available)"
else
	@echo "Platform: $(UNAME_S) (mock tests only)"
endif
	@echo ""
	@echo "Available targets:"
	@echo "  build          - Build debug binary"
	@echo "  build-release  - Build optimized release binary"
	@echo "  test           - Run tests (auto-detects platform)"
	@echo "  test-mock      - Run mock tests only (works on all platforms)"
	@echo "  test-all       - Run all tests (requires Linux/Raspberry Pi)"
	@echo "  test-verbose   - Run tests with output"
	@echo "  status         - Show platform and test capabilities"
	@echo "  clean          - Remove build artifacts"
	@echo "  run            - Run the application (debug)"
	@echo "  run-release    - Run the application (release)"
	@echo "  docker-build   - Build Docker image"
	@echo "  docker-run     - Run Docker Compose stack"
	@echo "  fmt            - Format code"
	@echo "  lint           - Run clippy linter"
	@echo "  check          - Check code without building"
	@echo "  install        - Install to system"
	@echo "  install-hooks  - Install git pre-commit hooks"
	@echo "  docs           - Generate documentation"
	@echo ""
	@echo "Workflows:"
	@echo "  dev            - Format, lint, and test"
	@echo "  ci             - CI checks with mock tests"
	@echo "  ci-full        - CI checks with all tests (Linux only)"
	@echo "  release        - Full release build"
	@echo ""
	@echo "Note: Matter/HomeKit integration has been removed."
	@echo "      See MATTER_INTEGRATION.md for re-integration instructions."

status:
	@echo "Platform Status"
	@echo "==============="
	@echo ""
	@echo "Operating System: $(UNAME_S)"
ifeq ($(UNAME_S),Linux)
	@echo "Hardware Support: FULL (Raspberry Pi GPIO, real sensors)"
	@echo "Test Mode: All tests will run (including hardware)"
	@echo ""
	@echo "Available:"
	@echo "  ✓ Mock tests"
	@echo "  ✓ Hardware GPIO tests"
	@echo "  ✓ Real sensor tests"
	@echo "  ✓ Full integration tests"
else
	@echo "Hardware Support: MOCK ONLY (cross-platform development)"
	@echo "Test Mode: Mock tests only (hardware tests skipped)"
	@echo ""
	@echo "Available:"
	@echo "  ✓ Mock tests"
	@echo "  ✗ Hardware GPIO tests (requires Linux)"
	@echo "  ✗ Real sensor tests (requires Linux)"
	@echo "  ⚠ Integration tests (using mocks)"
	@echo ""
	@echo "To run hardware tests, use Linux/Raspberry Pi or Docker"
endif
	@echo ""
	@echo "Matter Integration: DISABLED (see MATTER_INTEGRATION.md)"
	@echo ""
	@echo "Recommended commands:"
	@echo "  make test      - Auto-detect and run appropriate tests"
	@echo "  make test-mock - Always run mock tests (cross-platform)"
	@echo "  make dev       - Run development checks"

# Build targets
build:
	cargo build

build-release:
	cargo build --release

# Build for Raspberry Pi (ARM)
build-arm:
	cargo build --release --target armv7-unknown-linux-gnueabihf

build-arm64:
	cargo build --target aarch64-unknown-linux-gnu

build-release-arm64:
	cargo build --release --target aarch64-unknown-linux-gnu

# Test targets
test:
ifeq ($(UNAME_S),Linux)
	@echo "Linux detected - running all tests"
	cargo test
else
	@echo "Non-Linux platform ($(UNAME_S)) detected - running mock tests only"
	@echo "Note: Full hardware tests require Linux/Raspberry Pi"
	cargo test --test mock_integration_tests
endif

test-mock:
	@echo "Running mock tests (cross-platform)"
	cargo test --test mock_integration_tests

test-all:
	@echo "Running all tests (requires Linux/Raspberry Pi hardware)"
	cargo test

test-verbose:
ifeq ($(UNAME_S),Linux)
	@echo "Running all tests with output"
	cargo test -- --nocapture
else
	@echo "Running mock tests with output"
	cargo test --test mock_integration_tests -- --nocapture
endif

test-coverage:
	@echo "Generating code coverage report..."
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "cargo-llvm-cov not found. Installing..."; \
		cargo install cargo-llvm-cov; \
	}
	cargo llvm-cov --all-features --workspace --html --output-dir coverage
	@echo "Coverage report generated in coverage/index.html"

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
	rustup component add rustfmt clippy llvm-tools-preview
	cargo install cargo-llvm-cov
	@echo "Creating config from example..."
	@if [ ! -f config/config.json ]; then \
		cp config/config.example.json config/config.json; \
		echo "Created config/config.json - please edit with your settings"; \
	fi

# Install git hooks
install-hooks:
	@echo "Installing git pre-commit hook..."
	@if [ ! -f scripts/pre-commit ]; then \
		echo "❌ Error: scripts/pre-commit not found"; \
		exit 1; \
	fi
	@cp scripts/pre-commit .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@echo "✅ Git pre-commit hook installed successfully!"
	@echo ""
	@echo "The hook will automatically run before each commit:"
	@echo "  • cargo fmt (format check)"
	@echo "  • cargo clippy (lint check)"
	@echo "  • cargo test (run tests)"
	@echo ""
	@echo "To bypass the hook (emergency only): git commit --no-verify"

# Create systemd service
install-service:
	@echo "Installing systemd service..."
	sudo cp thermos-rust.service /etc/systemd/system/
	sudo systemctl daemon-reload
	sudo systemctl enable thermos-rust
	@echo "Service installed. Start with: sudo systemctl start thermos-rust"

# Development workflow
dev: fmt lint test
	@echo ""
	@echo "✓ Development checks complete!"
	@echo "  Platform: $(UNAME_S)"

# Release workflow
release: clean fmt lint test build-release
	@echo ""
	@echo "✓ Release build complete!"
	@echo "  Platform: $(UNAME_S)"
	@echo "  Binary at: target/release/thermos-rust"

# CI workflow (uses mock tests for cross-platform CI)
ci: fmt check lint test-mock
	@echo ""
	@echo "✓ CI checks complete!"
	@echo "  Platform: $(UNAME_S)"

# CI workflow for Linux/Raspberry Pi (runs all tests)
ci-full: fmt check lint test-all
	@echo ""
	@echo "✓ Full CI checks complete (with hardware tests)!"
	@echo "  Platform: $(UNAME_S)"
