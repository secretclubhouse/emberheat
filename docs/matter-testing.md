# Matter Testing with chip-tool

This guide covers testing the EmberHeat Matter thermostat using `chip-tool`, the Matter reference CLI.

## Installation

chip-tool must be built from source. There is no Homebrew package.

### macOS Prerequisites

1. Install Xcode from the Mac App Store
2. Install Xcode command line tools: `xcode-select --install`
3. For BLE pairing: Install the [Bluetooth Central Matter Client Developer Mode profile](https://developer.apple.com/download/all/?q=Bluetooth%20Central%20Matter%20Client%20Developer%20Mode)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/project-chip/connectedhomeip.git
cd connectedhomeip

# Initialize submodules (this takes a while)
./scripts/checkout_submodules.py --shallow --platform darwin

# Set up the build environment (run this in each new terminal)
source scripts/activate.sh

# Build chip-tool
./scripts/examples/gn_build_example.sh examples/chip-tool out/chip-tool

# Binary location
ls out/chip-tool/chip-tool
```

### Add to PATH (optional)

```bash
# Add to your shell profile (~/.zshrc or ~/.bashrc)
export PATH="$PATH:/path/to/connectedhomeip/out/chip-tool"
```

### Clean Up State Between Tests

chip-tool caches state in `/tmp`. Clean up between test sessions:
```bash
rm -rf /tmp/chip*
```

## Alternative: Python Matter Server

If building chip-tool is too involved, you can use Home Assistant's Python Matter Server which is easier to install:

```bash
# Install via pip
pip install python-matter-server

# Or use Home Assistant with the Matter integration
# which provides a web UI for commissioning and control
```

## Alternative: matter.js CLI

Another option is the matter.js CLI:

```bash
npm install -g @matter/cli
matter-controller --help
```

## Device Configuration

Default commissioning values (see `emberheat-esp32/src/config.rs`):
- **Discriminator:** 3840
- **Passcode:** 20202021
- **VID:** 0xFFF1 (test)
- **PID:** 0x8001 (test)

## Commissioning

### Wi-Fi (BLE-assisted)
```bash
chip-tool pairing ble-wifi <node-id> "<SSID>" "<password>" 20202021 3840
# Example:
chip-tool pairing ble-wifi 1 "MyNetwork" "MyPassword" 20202021 3840
```

### On-Network (already connected)
```bash
chip-tool pairing onnetwork <node-id> 20202021
# Example:
chip-tool pairing onnetwork 1 20202021
```

### Thread (BLE-assisted)
```bash
chip-tool pairing ble-thread <node-id> hex:<thread-dataset> 20202021 3840
```

### Unpair Device
```bash
chip-tool pairing unpair <node-id>
# Example:
chip-tool pairing unpair 1
```

## Endpoint Structure

| Endpoint | Purpose | Device Type |
|----------|---------|-------------|
| 0 | Root Node | 0x0016 |
| 1 | Zone 0 Thermostat | 0x0301 |

## Descriptor Cluster (0x001D)

Inspect device structure:

```bash
# List all endpoints
chip-tool descriptor read parts-list 1 0

# Device types on endpoint 1
chip-tool descriptor read device-type-list 1 1

# Server clusters on endpoint 1
chip-tool descriptor read server-list 1 1

# Client clusters on endpoint 1
chip-tool descriptor read client-list 1 1
```

## Basic Information Cluster (0x0028) - Endpoint 0

```bash
# Read all basic info
chip-tool basicinformation read-all 1 0

# Individual attributes
chip-tool basicinformation read vendor-id 1 0
chip-tool basicinformation read product-id 1 0
chip-tool basicinformation read vendor-name 1 0
chip-tool basicinformation read product-name 1 0
chip-tool basicinformation read software-version 1 0
chip-tool basicinformation read hardware-version 1 0
chip-tool basicinformation read serial-number 1 0
```

## Thermostat Cluster (0x0201) - Endpoint 1

### Read Operations

```bash
# Read all thermostat attributes
chip-tool thermostat read-all 1 1

# Current temperature (centi-degrees, e.g., 2150 = 21.50°C)
chip-tool thermostat read local-temperature 1 1

# Target heating setpoint
chip-tool thermostat read occupied-heating-setpoint 1 1

# System mode (0=Off, 4=Heat)
chip-tool thermostat read system-mode 1 1

# Control sequence (2=Heating Only)
chip-tool thermostat read control-sequence-of-operation 1 1

# Running state (bit 0 = heating active)
chip-tool thermostat read thermostat-running-state 1 1

# Feature map (0x01 = Heating feature)
chip-tool thermostat read feature-map 1 1

# Cluster revision
chip-tool thermostat read cluster-revision 1 1

# Setpoint limits
chip-tool thermostat read abs-min-heat-setpoint-limit 1 1
chip-tool thermostat read abs-max-heat-setpoint-limit 1 1
chip-tool thermostat read min-heat-setpoint-limit 1 1
chip-tool thermostat read max-heat-setpoint-limit 1 1
```

### Write Operations

```bash
# Set heating setpoint (centi-degrees)
# 20°C = 2000, 21°C = 2100, 22°C = 2200
chip-tool thermostat write occupied-heating-setpoint 2200 1 1

# Set system mode
# 0 = Off, 4 = Heat
chip-tool thermostat write system-mode 4 1 1
chip-tool thermostat write system-mode 0 1 1
```

### Temperature Conversion Reference

| Celsius | Centi-degrees |
|---------|---------------|
| 15°C | 1500 |
| 18°C | 1800 |
| 20°C | 2000 |
| 21°C | 2100 |
| 22°C | 2200 |
| 25°C | 2500 |

## Identify Cluster (0x0003) - Endpoint 1

```bash
# Read identify time (seconds remaining)
chip-tool identify read identify-time 1 1

# Read identify type
chip-tool identify read identify-type 1 1

# Trigger identify (seconds)
chip-tool identify identify 10 1 1
```

## Thermostat UI Configuration (0x0204) - Endpoint 1

```bash
# Read temperature display mode (0=Celsius, 1=Fahrenheit)
chip-tool thermostatuserinterfaceconfiguration read temperature-display-mode 1 1

# Read keypad lockout level
chip-tool thermostatuserinterfaceconfiguration read keypad-lockout 1 1

# Set to Fahrenheit
chip-tool thermostatuserinterfaceconfiguration write temperature-display-mode 1 1 1

# Set to Celsius
chip-tool thermostatuserinterfaceconfiguration write temperature-display-mode 0 1 1
```

## Binding Cluster (0x001E) - Endpoint 1

```bash
# Read current bindings
chip-tool binding read binding 1 1

# Write binding (for external temperature sensor)
# Format: [{FabricIndex, Node, Endpoint, Cluster}]
chip-tool binding write binding '[{"fabricIndex": 1, "node": 2, "endpoint": 1, "cluster": 1026}]' 1 1
```

## Subscription (Real-time Updates)

```bash
# Subscribe to temperature changes (min 1s, max 60s interval)
chip-tool thermostat subscribe local-temperature 1 60 1 1

# Subscribe to setpoint changes
chip-tool thermostat subscribe occupied-heating-setpoint 1 60 1 1

# Subscribe to running state
chip-tool thermostat subscribe thermostat-running-state 1 60 1 1
```

## Troubleshooting

### Commission Failed
```bash
# Factory reset device and retry
# Check device is in pairing mode (may need button press)
# Verify Wi-Fi credentials
# Ensure BLE is enabled on host
```

### Read/Write Timeout
```bash
# Device may have rebooted - recommission
chip-tool pairing unpair 1
chip-tool pairing onnetwork 1 20202021
```

### Invalid Command
```bash
# Check cluster is supported
chip-tool descriptor read server-list 1 1
```

### Attribute Not Found
```bash
# Verify attribute exists in cluster
chip-tool thermostat read-all 1 1
```

## Expected Values (Healthy Device)

| Attribute | Expected Value |
|-----------|----------------|
| device-type-list (ep1) | Contains 0x0301 |
| server-list (ep1) | 0x0003, 0x001D, 0x001E, 0x0201, 0x0204 |
| feature-map (thermostat) | 0x01 (Heating) |
| control-sequence | 2 (Heating Only) |
| system-mode | 0 (Off) or 4 (Heat) |
| local-temperature | 700-3700 (7-37°C valid range) |

## Compare with Reference Devices

Use [Matter Survey](https://matter-survey.org/) to compare cluster configurations:
- [Eve Thermo](https://matter-survey.org/device/eve-thermo-4874-79)

Submit your device for analysis once commissioned with any controller.
