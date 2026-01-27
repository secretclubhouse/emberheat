# Makefile for Thermos-Rust (ESP32-focused)

.PHONY: help build build-release test clean fmt lint check flash monitor install-hooks

# Default target
help:
	@echo "Thermos-Rust Build System (ESP32)"
	@echo ""
	@echo "Build targets:"
	@echo "  build          - Build debug (thermos-core only, ESP32 requires special toolchain)"
	@echo "  build-esp32    - Build ESP32 firmware (release)"
	@echo "  test           - Run tests (thermos-core)"
	@echo "  clean          - Remove build artifacts"
	@echo ""
	@echo "ESP32 targets:"
	@echo "  flash          - Build and flash ESP32 firmware"
	@echo "  monitor        - Open serial monitor"
	@echo "  flash-monitor  - Flash and open monitor"
	@echo ""
	@echo "Code quality:"
	@echo "  fmt            - Format code"
	@echo "  lint           - Run clippy linter (thermos-core)"
	@echo "  check          - Check code without building"
	@echo "  dev            - Format, lint, and test"
	@echo ""
	@echo "Setup:"
	@echo "  setup          - Install development dependencies"
	@echo "  install-hooks  - Install git pre-commit hooks"
	@echo ""
	@echo "Note: thermos-linux is preserved but not in active development."
	@echo "      Use 'cargo build -p thermos-linux' to build it explicitly."

# Build targets (thermos-core only by default, ESP32 needs cross-compilation)
build:
	cargo build -p thermos-core

build-release:
	cargo build --release -p thermos-core

# ESP32 build and flash
build-esp32:
	cd thermos-esp32 && cargo build --release

flash:
	cd thermos-esp32 && cargo build --release && espflash flash target/riscv32imac-unknown-none-elf/release/thermos-esp32

monitor:
	espflash monitor

flash-monitor:
	cd thermos-esp32 && cargo build --release && espflash flash --monitor target/riscv32imac-unknown-none-elf/release/thermos-esp32

# Test targets (thermos-core only)
test:
	cargo test -p thermos-core

test-verbose:
	cargo test -p thermos-core -- --nocapture

# Code quality targets (thermos-core only - ESP32 clippy requires cross-compilation)
fmt:
	cargo fmt

lint:
	cargo clippy -p thermos-core -- -D warnings

check:
	cargo check -p thermos-core

# Documentation
docs:
	cargo doc --no-deps -p thermos-core --open

# Clean
clean:
	cargo clean

# Setup development environment
setup:
	rustup component add rustfmt clippy
	@echo ""
	@echo "For ESP32 development, also install:"
	@echo "  rustup target add riscv32imac-unknown-none-elf"
	@echo "  cargo install espflash"

# Install git hooks
install-hooks:
	@echo "Installing git pre-commit hook..."
	@if [ ! -f scripts/pre-commit ]; then \
		echo "Error: scripts/pre-commit not found"; \
		exit 1; \
	fi
	@cp scripts/pre-commit .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@echo "Git pre-commit hook installed successfully!"

# Development workflow
dev: fmt lint test
	@echo ""
	@echo "Development checks complete!"
