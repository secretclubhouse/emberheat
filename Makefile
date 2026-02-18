# Makefile for EmberHeat (ESP32-focused)

.PHONY: help build build-release test clean fmt lint check flash monitor install-hooks
.PHONY: build-esp32 build-matter build-matter-dual-radio flash-matter flash-monitor-matter flash-matter-dual-radio flash-monitor-matter-dual-radio

# Load environment from .env.local or .env if they exist
ifneq (,$(wildcard .env.local))
    include .env.local
    export
else ifneq (,$(wildcard .env))
    include .env
    export
endif

# Default target
help:
	@echo "EmberHeat Build System (ESP32)"
	@echo ""
	@echo "Build targets:"
	@echo "  build            - Build debug (emberheat-core only, ESP32 requires special toolchain)"
	@echo "  build-esp32      - Build ESP32 firmware with MQTT (release)"
	@echo "  build-matter     - Build ESP32 firmware with Matter + MQTT (release)"
	@echo "  test             - Run tests (emberheat-core and emberheat-matter)"
	@echo "  clean            - Remove build artifacts"
	@echo ""
	@echo "ESP32 MQTT mode (requires WIFI_SSID and WIFI_PASSWORD):"
	@echo "  flash            - Build and flash ESP32 firmware (MQTT mode)"
	@echo "  flash-monitor    - Flash and open monitor (MQTT mode)"
	@echo ""
	@echo "ESP32 Matter mode (WiFi handled by Matter commissioning):"
	@echo "  flash-matter                   - Build and flash Matter firmware (single radio)"
	@echo "  flash-monitor-matter           - Flash Matter firmware and open monitor"
	@echo "  flash-matter-dual-radio        - Build and flash Matter with dual-radio (C6+H2)"
	@echo "  flash-monitor-matter-dual-radio - Flash dual-radio Matter and open monitor"
	@echo ""
	@echo "Other ESP32 targets:"
	@echo "  monitor          - Open serial monitor"
	@echo "  scan-sensors     - Flash and run DS18B20 sensor scanner"
	@echo ""
	@echo "Code quality:"
	@echo "  fmt              - Format code"
	@echo "  lint             - Run clippy linter (emberheat-core)"
	@echo "  check            - Check code without building"
	@echo "  dev              - Format, lint, and test"
	@echo ""
	@echo "Setup:"
	@echo "  setup            - Install development dependencies"
	@echo "  install-hooks    - Install git pre-commit hooks"
	@echo ""
	@echo "Environment variables (set in .env.local or export):"
	@echo "  WIFI_SSID        - WiFi network name (required for MQTT mode)"
	@echo "  WIFI_PASSWORD    - WiFi password (required for MQTT mode)"
	@echo "  MQTT_BROKER_HOST - MQTT broker hostname (default: localhost)"
	@echo "  MQTT_DEVICE_ID   - Device identifier (default: emberheat-esp32)"
	@echo "  MQTT_USERNAME    - MQTT auth username (optional)"
	@echo "  MQTT_PASSWORD    - MQTT auth password (optional)"
	@echo ""
	@echo "Note: Matter mode handles WiFi via BLE commissioning, so WIFI_SSID/PASSWORD"
	@echo "      are not required. MQTT settings are used in both modes."

# Build targets (emberheat-core only by default, ESP32 needs cross-compilation)
build:
	cargo build -p emberheat-core

build-release:
	cargo build --release -p emberheat-core

# ESP32 build and flash
# Note: Must cd into emberheat-esp32 to pick up .cargo/config.toml (target, build-std, linker flags)
# Output goes to workspace root target/ directory

# Helper to ensure WiFi credentials are set (prompts if not in env)
define check_wifi_creds
	@if [ -z "$$WIFI_SSID" ]; then \
		read -p "Enter WIFI_SSID: " ssid; \
		export WIFI_SSID="$$ssid"; \
	fi; \
	if [ -z "$$WIFI_PASSWORD" ]; then \
		read -p "Enter WIFI_PASSWORD: " pass; \
		export WIFI_PASSWORD="$$pass"; \
	fi
endef

# Build ESP32 firmware (MQTT mode - requires WiFi credentials)
build-esp32:
	$(check_wifi_creds)
	@cd emberheat-esp32 && WIFI_SSID="$$WIFI_SSID" WIFI_PASSWORD="$$WIFI_PASSWORD" cargo build --release

# Build ESP32 firmware with Matter + MQTT (WiFi via Matter commissioning)
# Note: WiFi credentials not needed - Matter handles WiFi via BLE commissioning
# MQTT broker settings are still used for telemetry
build-matter:
	@cd emberheat-esp32 && cargo build --features matter --release

# Build ESP32 firmware with Matter using dual-radio (C6 WiFi + H2 BLE over UART)
# Requires ESP32-H2 with HCI UART firmware connected to C6 via UART
build-matter-dual-radio:
	@cd emberheat-esp32 && cargo build --features "matter,external-ble" --release

# Flash targets (MQTT mode)
flash:
	$(check_wifi_creds)
	@cd emberheat-esp32 && WIFI_SSID="$$WIFI_SSID" WIFI_PASSWORD="$$WIFI_PASSWORD" cargo build --release
	espflash flash target/riscv32imac-unknown-none-elf/release/emberheat-esp32

flash-monitor:
	$(check_wifi_creds)
	@cd emberheat-esp32 && WIFI_SSID="$$WIFI_SSID" WIFI_PASSWORD="$$WIFI_PASSWORD" cargo build --release
	espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32

# Flash targets (Matter mode - single radio)
flash-matter: build-matter
	espflash flash target/riscv32imac-unknown-none-elf/release/emberheat-esp32

flash-monitor-matter: build-matter
	espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32

# Flash targets (Matter mode - dual radio with ESP32-H2)
flash-matter-dual-radio: build-matter-dual-radio
	espflash flash target/riscv32imac-unknown-none-elf/release/emberheat-esp32

flash-monitor-matter-dual-radio: build-matter-dual-radio
	espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32

# Serial monitor
monitor:
	espflash monitor

# Scan for DS18B20 sensors on the 1-Wire bus
scan-sensors:
	@cd emberheat-esp32 && WIFI_SSID="unused" WIFI_PASSWORD="unused" cargo build --release --bin scan_sensors
	espflash flash --monitor target/riscv32imac-unknown-none-elf/release/scan_sensors

# Test targets (emberheat-core and emberheat-matter)
test:
	cargo test -p emberheat-core -p emberheat-matter

test-verbose:
	cargo test -p emberheat-core -p emberheat-matter -- --nocapture

# Code quality targets (emberheat-core only - ESP32 clippy requires cross-compilation)
fmt:
	cargo fmt

lint:
	cargo clippy -p emberheat-core -- -D warnings

check:
	cargo check -p emberheat-core

# Documentation
docs:
	cargo doc --no-deps -p emberheat-core --open

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
	@echo ""
	@echo "Then copy .env.example to .env.local and configure your settings."

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
