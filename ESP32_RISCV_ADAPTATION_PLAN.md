# ESP32 RISC-V Adaptation Plan

## Executive Summary

This document outlines the comprehensive plan to adapt the thermos-rust codebase from Linux/Raspberry Pi (ARM) to ESP32 RISC-V microcontrollers. The project is currently a multi-zone HomeKit thermostat using Matter protocol, designed for Linux with full OS support. Adapting to ESP32 requires fundamental architectural changes to work within embedded constraints.

**Estimated Total Effort:** 850-1,350 hours across 6-8 months

**Complexity:** High - Major architectural refactoring required

---

## Table of Contents

1. [Current Architecture Analysis](#1-current-architecture-analysis)
2. [Target ESP32 RISC-V Platforms](#2-target-esp32-risc-v-platforms)
3. [Core Challenges](#3-core-challenges)
4. [Adaptation Strategy](#4-adaptation-strategy)
5. [Detailed Implementation Plan](#5-detailed-implementation-plan)
6. [Testing Strategy](#6-testing-strategy)
7. [Risk Assessment](#7-risk-assessment)
8. [Timeline and Milestones](#8-timeline-and-milestones)

---

## 1. Current Architecture Analysis

### 1.1 Platform-Specific Components (~1,200 lines)

#### GPIO Module (`src/gpio/mod.rs` - 372 lines)
- **Current:** Direct dependency on `rppal` (Raspberry Pi Peripheral Access Library)
- **Issue:** Hardcoded `rppal::gpio::Gpio`, `OutputPin`, `Level` types throughout
- **Usage:** Controls heating relays (active-LOW logic) and sensor power (VCC pin)
- **Location:** Lines 35, 84, 136, 206

#### 1-Wire Sensor (`src/sensors/onewire.rs` - 222 lines)
- **Current:** Reads from Linux sysfs `/sys/bus/w1/devices/`
- **Issue:** No 1-Wire driver exists in esp-hal, would need custom implementation
- **Protocol:** DS18B20 temperature sensors
- **Location:** Line 15 - hardcoded path

#### Configuration (`src/config/mod.rs` - 470 lines)
- **Current:** Reads from Unix filesystem with HOME variable expansion
- **Issue:** ESP32 has no concept of HOME, limited filesystem options
- **Storage:** JSON files, search paths in `/etc/`, `~/.config/`, local dirs
- **Location:** main.rs lines 73-104

#### Signal Handling (`src/main.rs`)
- **Current:** Unix signals (SIGTERM, SIGINT) via `signal-hook-tokio`
- **Issue:** No signal support on ESP32
- **Location:** Lines 46, 161

### 1.2 Platform-Independent Components (~2,500 lines)

✅ **Can be reused with minimal changes:**

- **Control Logic** (`src/devices/mod.rs` - 581 lines): Pure Rust, no hardware deps
- **Sensor Abstractions** (`src/sensors/mod.rs`): Trait-based, extensible
- **Redis Persistence** (333 lines): Network-based, ESP32 compatible
- **Prometheus Metrics** (279 lines): HTTP server, needs async runtime swap
- **Matter Protocol** (928 lines): FFI to C library, needs ESP32 Matter SDK

### 1.3 Current Dependencies

```toml
tokio = { version = "1.48", features = ["full"] }  # ❌ Not embedded-compatible
rppal = "0.22"                                      # ❌ Linux-only
signal-hook = "0.3"                                 # ❌ Unix-only
reqwest = "0.12"                                    # ❌ Needs tokio runtime
redis = { version = "0.27", features = ["tokio-comp"] }  # ⚠️ Needs async runtime
```

---

## 2. Target ESP32 RISC-V Platforms

### 2.1 Recommended Chips

| Chip | Cores | Clock | SRAM | Flash | WiFi | Thread | Use Case |
|------|-------|-------|------|-------|------|--------|----------|
| **ESP32-C6** ⭐ | 1 RISC-V | 160 MHz | 512 KB | 4 MB | Yes | Yes | **Best choice** - Thread support for Matter |
| **ESP32-C3** | 1 RISC-V | 160 MHz | 400 KB | 4 MB | Yes | No | Budget option, WiFi Matter only |
| **ESP32-H2** | 1 RISC-V | 96 MHz | 320 KB | 4 MB | No | Yes | Thread-only, low power |

**Recommendation:** **ESP32-C6** for production (Thread + WiFi for Matter)

### 2.2 Resource Constraints

| Resource | Raspberry Pi 4 | ESP32-C6 | Impact |
|----------|---------------|----------|--------|
| RAM | 1-8 GB | 512 KB | **Critical** - 2000x less memory |
| CPU | 1.5 GHz ARM Cortex-A72 | 160 MHz RISC-V | **High** - 10x slower |
| Storage | MicroSD (GB) | 4 MB flash | **High** - No dynamic config loading |
| OS | Linux | Bare metal | **Critical** - No filesystem, signals, processes |

---

## 3. Core Challenges

### 3.1 Async Runtime Incompatibility

**Problem:** Tokio requires `std` and OS threads, not available on `no_std` embedded

**Current usage:**
```rust
#[tokio::main]
async fn main() -> Result<()> {
    tokio::spawn(async move { ... });
    tokio::time::sleep(duration).await;
}
```

**Solution:** Replace with Embassy (embedded async runtime)

**Impact:** ~40% of codebase uses tokio APIs

### 3.2 GPIO Hardware Abstraction

**Problem:** Direct rppal coupling in 372 lines

**Current:**
```rust
use rppal::gpio::{Gpio, OutputPin, Level};
let gpio = Gpio::new()?;
let pin = gpio.get(23)?.into_output();
```

**Required:**
```rust
// ESP32-HAL version
use esp_hal::gpio::{GpioPin, Output, PushPull};
let peripherals = Peripherals::take();
let pin = Output::new(peripherals.GPIO23, Level::Low);
```

**Impact:** Full refactor of GPIO module + thermostat integration

### 3.3 1-Wire Protocol

**Problem:** No 1-Wire driver in esp-hal

**Current:** Linux kernel driver via sysfs
**Required:** Implement 1-Wire protocol using:
- RMT peripheral (precision timing)
- Or bit-banging with interrupts

**Complexity:** Medium - protocol is well-documented

### 3.4 Matter SDK Integration

**Problem:** Connected Home IP (Matter SDK) needs ESP32 build

**Current:** Git submodule with FFI bindings
**Required:**
1. ESP-IDF build system integration
2. Thread/WiFi commissioning for ESP32-C6
3. Credential storage in NVS (Non-Volatile Storage)

**Complexity:** High - 100-200 hours

### 3.5 Memory Constraints

**Problem:** 512 KB SRAM vs GB of Linux RAM

**Issues:**
- Full tokio runtime: ~200 KB stack
- Matter SDK: ~150 KB
- TLS/crypto: ~50 KB
- Application: ~100 KB
- **Total: ~500 KB** (no room for error)

**Mitigation:**
- Use static allocation where possible
- Optimize Matter SDK build (disable unused features)
- Consider external PSRAM (ESP32-C6 supports up to 16 MB)

---

## 4. Adaptation Strategy

### 4.1 Phased Migration Approach

**Phase 1: Hardware Abstraction (Months 1-2)**
- Create HAL trait layer
- Dual implementation: rppal + esp-hal
- Feature flags for platform selection

**Phase 2: Async Runtime (Months 2-4)**
- Replace tokio with embassy
- Port all async code
- Network stack integration (esp-wifi)

**Phase 3: Sensors & Peripherals (Months 4-5)**
- Implement 1-Wire driver for ESP32
- Port sensor abstractions
- Add ESP32-specific sensor types

**Phase 4: Matter Integration (Months 5-7)**
- ESP-IDF Matter SDK build
- Thread commissioning
- NVS credential storage

**Phase 5: Testing & Optimization (Months 7-8)**
- Hardware-in-loop testing
- Memory profiling
- Performance tuning

### 4.2 Feature Flag Architecture

```toml
[features]
default = ["rpi"]
rpi = ["rppal", "tokio"]
esp32-c6 = ["esp-hal", "embassy-executor", "esp-wifi"]
esp32-c3 = ["esp-hal", "embassy-executor", "esp-wifi"]

[dependencies]
# Conditional dependencies
rppal = { version = "0.22", optional = true }
embassy-executor = { version = "0.6", optional = true }
esp-hal = { version = "0.20", optional = true }
```

---

## 5. Detailed Implementation Plan

### 5.1 GPIO Abstraction Layer (150-200 hours)

**Step 1: Define HAL Trait**
```rust
// src/hal/gpio.rs
pub trait GpioHal {
    type OutputPin: OutputPin;
    type InputPin: InputPin;

    fn get_output(&self, pin: u8) -> Result<Self::OutputPin>;
    fn get_input(&self, pin: u8) -> Result<Self::InputPin>;
}

pub trait OutputPin {
    fn set_high(&mut self);
    fn set_low(&mut self);
    fn is_set_high(&self) -> bool;
    fn is_set_low(&self) -> bool;
}
```

**Step 2: Implement for Raspberry Pi**
```rust
// src/hal/gpio_rpi.rs
#[cfg(feature = "rpi")]
pub struct RpiGpioHal {
    gpio: rppal::gpio::Gpio,
}

// Wrapper around rppal::gpio::OutputPin
pub struct RpiOutputPin {
    inner: rppal::gpio::OutputPin,
}
```

**Step 3: Implement for ESP32**
```rust
// src/hal/gpio_esp32.rs
#[cfg(feature = "esp32-c6")]
pub struct Esp32GpioHal {
    // Store peripherals or pin references
}

pub struct Esp32OutputPin<'a, P> {
    inner: esp_hal::gpio::Output<'a, P>,
}
```

**Step 4: Update GpioController**
```rust
// src/gpio/mod.rs
use crate::hal::gpio::{GpioHal, OutputPin};

pub struct GpioController<H: GpioHal> {
    hal: H,
    relays: Arc<Mutex<HashMap<u8, H::OutputPin>>>,
}
```

**Files to modify:**
- `src/gpio/mod.rs` (372 lines) - Complete refactor
- `src/devices/mod.rs` (30 lines) - Update thermostat to use HAL
- Create `src/hal/mod.rs`, `src/hal/gpio.rs`, `src/hal/gpio_rpi.rs`, `src/hal/gpio_esp32.rs`

**Estimated:** 150-200 hours

---

### 5.2 Async Runtime Migration (200-400 hours)

**Tokio → Embassy mapping:**

| Tokio | Embassy | Notes |
|-------|---------|-------|
| `#[tokio::main]` | `#[embassy_executor::main]` | Entry point |
| `tokio::spawn()` | `spawner.spawn()` | Task spawning |
| `tokio::time::sleep()` | `embassy_time::Timer::after()` | Delays |
| `tokio::select!` | `embassy_futures::select::select()` | Multiplexing |
| `tokio::sync::Mutex` | `embassy_sync::mutex::Mutex` | No std mutex |

**Step 1: Update main.rs**
```rust
// Current
#[tokio::main]
async fn main() -> Result<()> {
    tokio::spawn(async move { metrics_server.run().await });
}

// ESP32 version
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    spawner.spawn(metrics_task(metrics_server)).ok();
}
```

**Step 2: Replace time operations**
```rust
// Current
use tokio::time::{sleep, timeout, Duration};
sleep(Duration::from_secs(5)).await;

// ESP32
use embassy_time::{Duration, Timer};
Timer::after(Duration::from_secs(5)).await;
```

**Step 3: Update async traits**
```rust
// src/sensors/mod.rs - TemperatureSensor trait
#[async_trait]  // Remove this
pub trait TemperatureSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading>;
}
```

**Files to modify:**
- `src/main.rs` (237 lines) - Complete rewrite
- `src/devices/mod.rs` (581 lines) - Update all async/await
- `src/sensors/*.rs` (500 lines) - Replace tokio::fs, timeout
- `src/persistence/mod.rs` (333 lines) - Redis client compat check
- `src/metrics/mod.rs` (279 lines) - HTTP server replacement

**Estimated:** 200-400 hours (depends on Redis/HTTP lib availability)

---

### 5.3 1-Wire Driver Implementation (150-200 hours)

**Option A: RMT Peripheral (Recommended)**

ESP32 RMT (Remote Control) peripheral can generate precise 1-Wire timing:

```rust
// src/hal/onewire_esp32.rs
use esp_hal::rmt::{Rmt, TxChannel};

pub struct Esp32OneWire<'d, T: TxChannel> {
    rmt: Rmt<'d>,
    channel: T,
}

impl Esp32OneWire {
    // 1-Wire reset pulse: 480μs LOW, 70μs HIGH
    async fn reset(&mut self) -> Result<bool> {
        let pulse = RmtPulse::new(480, 70);
        self.channel.transmit(pulse).await?;
        // Check for presence pulse
    }

    // Write bit: 60μs LOW for 0, 6μs LOW for 1
    async fn write_bit(&mut self, bit: bool) -> Result<()> {
        let duration = if bit { 6 } else { 60 };
        let pulse = RmtPulse::new(duration, 60 - duration);
        self.channel.transmit(pulse).await
    }

    // Read DS18B20 temperature
    pub async fn read_temperature(&mut self, rom: [u8; 8]) -> Result<f32> {
        self.reset().await?;
        self.write_byte(0x55).await?; // Match ROM
        self.write_rom(&rom).await?;
        self.write_byte(0x44).await?; // Start conversion
        embassy_time::Timer::after_millis(750).await; // Wait for conversion
        // Read scratchpad...
    }
}
```

**Option B: Bit-Banging (Fallback)**

Manual GPIO toggling with precise timing (less reliable):
```rust
pub struct BitBangOneWire<P: OutputPin> {
    pin: P,
}

impl BitBangOneWire {
    async fn write_bit(&mut self, bit: bool) {
        self.pin.set_low();
        if bit {
            embassy_time::Timer::after_micros(6).await;
        } else {
            embassy_time::Timer::after_micros(60).await;
        }
        self.pin.set_high();
        embassy_time::Timer::after_micros(60).await;
    }
}
```

**Step 1: Implement 1-Wire protocol**
- Create `src/hal/onewire_esp32.rs` (300-400 lines)
- Implement reset, write_bit, read_bit, write_byte, read_byte
- Add DS18B20-specific commands (match ROM, skip ROM, convert T, read scratchpad)

**Step 2: Update OneWireSensor**
```rust
// src/sensors/onewire.rs
#[cfg(feature = "rpi")]
const ONEWIRE_BASE_PATH: &str = "/sys/bus/w1/devices";

#[cfg(feature = "esp32-c6")]
use crate::hal::onewire_esp32::Esp32OneWire;

pub struct OneWireSensor {
    #[cfg(feature = "rpi")]
    device_path: PathBuf,

    #[cfg(feature = "esp32-c6")]
    onewire: Esp32OneWire,
    rom_code: [u8; 8],
}
```

**Estimated:** 150-200 hours (testing on hardware is critical)

---

### 5.4 Configuration & Storage (50-100 hours)

**Problem:** No filesystem, no HOME variable

**Solution:** ESP32 NVS (Non-Volatile Storage)

```rust
// src/config_esp32.rs
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};

pub struct Esp32ConfigStore {
    nvs: EspNvs<NvsDefault>,
}

impl Esp32ConfigStore {
    pub fn load_config() -> Result<Config> {
        let nvs = EspNvs::new(NvsDefault::take()?, "thermos", true)?;

        // Read JSON blob from NVS
        let mut buf = vec![0u8; 4096];
        let len = nvs.get_blob("config", &mut buf)?;
        let config: Config = serde_json::from_slice(&buf[..len])?;
        Ok(config)
    }

    pub fn save_config(config: &Config) -> Result<()> {
        let json = serde_json::to_vec(config)?;
        nvs.set_blob("config", &json)?;
        Ok(())
    }
}
```

**Alternative: Compile-time configuration**
```rust
// config/esp32_config.rs
pub const CONFIG: Config = Config {
    zones: &[
        ZoneConfig {
            name: "living_room",
            relay_pin: 23,
            sensor_id: "28-00000ab12345",
            // ...
        }
    ],
};
```

**Recommendation:** Hybrid approach
- Compile-time defaults
- NVS for runtime overrides (set points, schedules)
- HTTP API for configuration (Matter doesn't support all settings)

**Estimated:** 50-100 hours

---

### 5.5 Matter SDK Integration (100-200 hours)

**Current Setup:**
- Git submodule: `third_party/connectedhomeip`
- FFI bindings in `src/matter/ffi.rs`
- Build script: `build.rs`

**ESP32 Requirements:**

1. **Build System Changes**
```bash
# Need ESP-IDF toolchain
idf.py set-target esp32c6
idf.py build

# Matter SDK for ESP32
cd third_party/connectedhomeip
source scripts/activate.sh
./scripts/examples/esp_example.sh esp32c6
```

2. **ESP-IDF Component**
```cmake
# components/matter_wrapper/CMakeLists.txt
idf_component_register(
    SRCS "matter_wrapper.c"
    INCLUDE_DIRS "include"
    REQUIRES chip nvs_flash esp_wifi
)
```

3. **Credential Storage**
```rust
// Store commissioning credentials in NVS
use esp_idf_svc::nvs::EspNvs;

pub struct MatterCredentials {
    setup_code: String,  // QR code data
    discriminator: u16,
    vendor_id: u16,
    product_id: u16,
}

impl MatterCredentials {
    pub fn load_from_nvs() -> Result<Self> { /* ... */ }
    pub fn store_to_nvs(&self) -> Result<()> { /* ... */ }
}
```

4. **Thread Commissioning (ESP32-C6)**
```rust
// Enable Thread radio
use esp_idf_svc::thread::EspThread;

pub async fn init_matter_thread() -> Result<()> {
    let thread = EspThread::new()?;
    thread.start()?;

    // Matter SDK will handle Thread commissioning
    matter_server.enable_thread_transport()?;
    Ok(())
}
```

**Files to modify:**
- `build.rs` - Add ESP-IDF build logic
- `src/matter/mod.rs` (928 lines) - Update FFI for ESP32
- Create `src/matter/esp32.rs` - ESP32-specific initialization

**Estimated:** 100-200 hours (Matter SDK complexity)

---

### 5.6 Network Stack (50-100 hours)

**Replace reqwest + redis with ESP32 equivalents**

**HTTP Client:**
```rust
// Current: reqwest
let response = reqwest::get(url).await?.json::<Response>().await?;

// ESP32: esp-idf-svc HTTP client
use esp_idf_svc::http::client::EspHttpConnection;

let http = EspHttpConnection::new(&Configuration::default())?;
let response = http.get(url)?;
```

**Redis Client:**

Options:
1. **Port redis-rs to embassy** (if tokio-comp can be swapped)
2. **Use raw TCP socket** (Redis protocol is text-based)
3. **HTTP fallback** (REST API instead of Redis)

Example raw Redis:
```rust
use embassy_net::tcp::TcpSocket;

async fn redis_set(key: &str, value: &str) -> Result<()> {
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.connect((redis_addr, 6379)).await?;

    let cmd = format!("SET {} {}\r\n", key, value);
    socket.write_all(cmd.as_bytes()).await?;

    // Parse response
    let mut buf = [0u8; 1024];
    let n = socket.read(&mut buf).await?;
    // Check for "+OK\r\n"
    Ok(())
}
```

**Estimated:** 50-100 hours

---

## 6. Testing Strategy

### 6.1 Unit Tests

**Maintain existing tests with conditional compilation:**
```rust
#[cfg(test)]
mod tests {
    #[cfg(feature = "rpi")]
    #[test]
    fn test_rpi_gpio() { /* ... */ }

    #[cfg(feature = "esp32-c6")]
    #[test]
    fn test_esp32_gpio() { /* ... */ }
}
```

### 6.2 Hardware-in-Loop (HIL) Testing

**Setup:**
1. ESP32-C6 DevKit
2. DS18B20 sensor breakout
3. Relay module (5V active-LOW)
4. USB serial connection for logging

**Test Cases:**
- GPIO relay control (ON/OFF states)
- 1-Wire temperature reading accuracy
- Matter commissioning (Thread + WiFi)
- Multi-zone coordination
- Power cycling behavior
- OTA updates

### 6.3 Simulator Testing

**Use esp-hal simulator for basic tests:**
```rust
#[cfg(all(test, feature = "esp32-c6"))]
mod sim_tests {
    use esp_hal_sim::Simulator;

    #[test]
    fn test_gpio_output() {
        let sim = Simulator::new();
        let hal = Esp32GpioHal::new(sim.peripherals());
        // Test GPIO operations
    }
}
```

---

## 7. Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Memory overflow** | High | Critical | Add PSRAM, optimize Matter SDK, static allocation |
| **1-Wire timing issues** | Medium | High | Use RMT peripheral, extensive testing |
| **Matter SDK incompatibility** | Medium | High | Early prototyping, Espressif support |
| **Performance bottleneck** | Medium | Medium | Profiling, optimize hot paths |
| **Embassy maturity** | Low | High | Active community, stable releases |
| **Thread certification** | Low | Medium | Follow Espressif examples |

---

## 8. Timeline and Milestones

### Month 1-2: Foundation
- ✅ GPIO HAL trait definition
- ✅ Raspberry Pi HAL implementation
- ✅ ESP32 HAL implementation
- ✅ Basic relay control working on ESP32

**Deliverable:** ESP32 can control relays via HAL

### Month 2-4: Async Runtime
- ✅ Embassy executor integration
- ✅ Replace tokio::time with embassy_time
- ✅ Port all async code
- ✅ Network stack (WiFi) working

**Deliverable:** ESP32 runs async tasks, connects to WiFi

### Month 4-5: Sensors
- ✅ 1-Wire driver (RMT peripheral)
- ✅ DS18B20 temperature reading
- ✅ Sensor abstraction updates
- ✅ HTTP sensor fallback

**Deliverable:** ESP32 reads temperature sensors

### Month 5-7: Matter Integration
- ✅ ESP-IDF build system
- ✅ Matter SDK compilation for ESP32-C6
- ✅ Thread commissioning
- ✅ HomeKit pairing

**Deliverable:** ESP32 appears in HomeKit as thermostat

### Month 7-8: Testing & Polish
- ✅ Hardware-in-loop testing
- ✅ Memory optimization
- ✅ Multi-zone testing
- ✅ Documentation
- ✅ OTA update support

**Deliverable:** Production-ready ESP32 firmware

---

## 9. Code Structure Changes

### 9.1 New Directory Layout

```
thermos-rust/
├── src/
│   ├── hal/                    # NEW: Hardware abstraction layer
│   │   ├── mod.rs
│   │   ├── gpio.rs             # GPIO trait definitions
│   │   ├── gpio_rpi.rs         # Raspberry Pi implementation
│   │   ├── gpio_esp32.rs       # ESP32 implementation
│   │   ├── onewire.rs          # 1-Wire trait
│   │   ├── onewire_rpi.rs      # Linux sysfs implementation
│   │   └── onewire_esp32.rs    # RMT-based implementation
│   ├── platforms/              # NEW: Platform-specific code
│   │   ├── rpi/
│   │   │   ├── main.rs         # Linux main with tokio
│   │   │   └── config.rs       # Filesystem config
│   │   └── esp32/
│   │       ├── main.rs         # Embassy main
│   │       ├── config.rs       # NVS config
│   │       └── wifi.rs         # WiFi setup
│   ├── gpio/
│   │   └── mod.rs              # REFACTORED: Uses HAL traits
│   ├── sensors/
│   │   ├── mod.rs
│   │   ├── onewire.rs          # REFACTORED: Uses HAL onewire
│   │   └── http.rs             # UPDATED: Embassy http client
│   ├── devices/
│   │   └── mod.rs              # UPDATED: Embassy async
│   ├── matter/
│   │   ├── mod.rs
│   │   ├── ffi.rs
│   │   └── esp32.rs            # NEW: ESP32-specific Matter code
│   ├── persistence/
│   │   ├── mod.rs
│   │   └── esp32.rs            # NEW: NVS storage option
│   └── main.rs                 # REFACTORED: Conditional compilation
├── Cargo.toml                  # UPDATED: Feature flags
├── .cargo/
│   └── config.toml             # NEW: ESP32 build targets
├── sdkconfig.defaults          # NEW: ESP-IDF configuration
└── rust-toolchain.toml         # NEW: ESP32 toolchain
```

### 9.2 Cargo.toml Changes

```toml
[package]
name = "thermos-rust"
version = "0.2.0"  # Bump version for ESP32 support

[features]
default = ["rpi"]
rpi = ["rppal", "tokio", "signal-hook"]
esp32-c6 = ["esp32-hal", "embassy"]
esp32-c3 = ["esp32-hal", "embassy"]

[dependencies]
# Shared dependencies (work on both platforms)
serde = { version = "1.0", default-features = false, features = ["derive"] }
serde_json = { version = "1.0", default-features = false }
anyhow = { version = "1.0", default-features = false }
embedded-hal = "1.0"  # Trait definitions

# Platform-specific: Raspberry Pi
rppal = { version = "0.22", optional = true }
tokio = { version = "1.48", features = ["full"], optional = true }
signal-hook = { version = "0.3", optional = true }

# Platform-specific: ESP32
esp-hal = { version = "0.20", optional = true }
esp-backtrace = { version = "0.14", optional = true }
embassy-executor = { version = "0.6", optional = true }
embassy-time = { version = "0.3", optional = true }
embassy-sync = { version = "0.6", optional = true }
embassy-net = { version = "0.4", optional = true }
esp-wifi = { version = "0.9", optional = true }
esp-idf-svc = { version = "0.49", optional = true }

[target.'cfg(target_arch = "riscv32")'.dependencies]
# ESP32 RISC-V specific
riscv = "0.11"

[[bin]]
name = "thermos-rust"
path = "src/main.rs"
required-features = []  # Can build with either rpi or esp32 features
```

---

## 10. Alternative Architectures

### 10.1 Hybrid Approach

**Keep Raspberry Pi as coordinator, ESP32 as I/O nodes**

Pros:
- Simpler adaptation (less code changes)
- Leverage RPi's resources for Matter stack
- ESP32 handles GPIO/sensors only
- Easier debugging

Cons:
- More hardware required
- Network latency between RPi and ESP32
- Single point of failure (RPi)

Architecture:
```
[Raspberry Pi]
  ├─ Matter Server
  ├─ Redis
  ├─ Prometheus
  └─ HTTP API
       ↓ (WiFi/Ethernet)
[ESP32 Node 1]  [ESP32 Node 2]  [ESP32 Node 3]
  ├─ GPIO         ├─ GPIO         ├─ GPIO
  └─ 1-Wire       └─ 1-Wire       └─ 1-Wire
```

### 10.2 ESP32-S3 (Xtensa) Alternative

If RISC-V is not a hard requirement:

**ESP32-S3:**
- Dual-core Xtensa @ 240 MHz
- 512 KB SRAM (up to 32 MB PSRAM)
- WiFi + Bluetooth
- More mature toolchain
- Similar code changes required

**Consideration:** All the async runtime and HAL work applies equally to S3

---

## 11. Success Criteria

### 11.1 Functional Requirements

✅ **Must Have:**
- [ ] ESP32-C6 controls relays (ON/OFF)
- [ ] Reads DS18B20 temperature sensors
- [ ] Connects to HomeKit via Matter (Thread)
- [ ] Multi-zone support (minimum 2 zones)
- [ ] Hysteresis control logic
- [ ] Persists state across reboots (NVS)

⚠️ **Should Have:**
- [ ] Prometheus metrics export
- [ ] Redis persistence
- [ ] HTTP sensor fallback
- [ ] OTA updates
- [ ] WiFi fallback (when Thread unavailable)

❌ **Nice to Have:**
- [ ] Web UI for configuration
- [ ] MQTT integration
- [ ] Energy monitoring
- [ ] Scheduling

### 11.2 Performance Requirements

- Temperature read latency: < 1 second
- Relay response time: < 100 ms
- Matter command latency: < 500 ms
- Memory usage: < 400 KB (leave headroom)
- Uptime: > 99.9% (< 9 hours downtime/year)

### 11.3 Quality Requirements

- Unit test coverage: > 70%
- HIL test pass rate: 100%
- Matter certification: Pass
- Thread certification: Pass (ESP32-C6)
- Documentation: Complete API docs + user guide

---

## 12. Next Steps

### Immediate Actions (Week 1)

1. **Hardware procurement:**
   - Order ESP32-C6 DevKit (x3 for testing)
   - DS18B20 sensors (x5)
   - 5V relay modules (x3)
   - Thread border router (if needed)

2. **Development environment setup:**
   ```bash
   # Install ESP-IDF
   git clone --recursive https://github.com/espressif/esp-idf.git
   cd esp-idf
   ./install.sh esp32c6
   . ./export.sh

   # Install Rust ESP32 toolchain
   cargo install espup
   espup install
   . ~/export-esp.sh

   # Install cargo-espflash
   cargo install cargo-espflash
   ```

3. **Repository setup:**
   ```bash
   # Create feature branch
   git checkout -b feature/esp32-support

   # Add ESP32 config files
   touch .cargo/config.toml
   touch sdkconfig.defaults
   touch rust-toolchain.toml
   ```

4. **Proof of concept:**
   - Create minimal ESP32-C6 blink example
   - Test GPIO control (relay ON/OFF)
   - Verify RMT peripheral for 1-Wire timing

### Week 2-4: HAL Foundation

- Implement GPIO HAL trait
- Create both RPI and ESP32 implementations
- Unit tests for HAL
- Refactor GpioController to use HAL

### Month 2: Async Runtime

- Set up embassy-executor
- Port main event loop
- Replace tokio::time calls
- Test thermostat control logic with embassy

---

## 13. Resources & References

### Documentation

- **ESP32-C6 Technical Reference:** https://www.espressif.com/en/products/socs/esp32-c6
- **esp-hal Documentation:** https://docs.esp-rs.org/esp-hal/
- **Embassy Book:** https://embassy.dev/book/
- **Matter Specification:** https://csa-iot.org/developer-resource/specifications-download-request/
- **DS18B20 Datasheet:** https://www.analog.com/media/en/technical-documentation/data-sheets/DS18B20.pdf

### Example Projects

- **esp32-matter-example:** https://github.com/espressif/esp-matter
- **embassy-examples:** https://github.com/embassy-rs/embassy/tree/main/examples
- **esp-hal-examples:** https://github.com/esp-rs/esp-hal/tree/main/examples

### Community Support

- **ESP-RS Matrix:** https://matrix.to/#/#esp-rs:matrix.org
- **Embassy Matrix:** https://matrix.to/#/#embassy-rs:matrix.org
- **Rust Embedded:** https://matrix.to/#/#rust-embedded:matrix.org

---

## Appendix A: Memory Budget Analysis

### Current Linux Version (No Constraints)

| Component | Size | Notes |
|-----------|------|-------|
| Tokio runtime | ~200 KB | Full runtime with thread pool |
| Matter SDK | ~150 KB | Complete implementation |
| Application code | ~50 KB | All modules |
| Stack | ~100 KB | Multiple OS threads |
| Heap | Unlimited | Dynamic allocation |

### ESP32-C6 Version (512 KB SRAM)

| Component | Size | Notes |
|-----------|------|-------|
| Embassy executor | ~20 KB | Minimal async runtime |
| Matter SDK | ~120 KB | Optimized build, Thread only |
| WiFi stack | ~50 KB | esp-wifi library |
| TLS/crypto | ~40 KB | mbedtls |
| Application code | ~30 KB | Embedded optimizations |
| Static buffers | ~50 KB | Network, sensors, logs |
| Stack | ~40 KB | Main task + spawned tasks |
| Heap | ~100 KB | Dynamic allocation |
| **Reserved** | ~62 KB | Safety margin (12%) |
| **TOTAL** | **512 KB** | At capacity |

**Mitigation if over budget:**
- Add external PSRAM (ESP32-C6 supports up to 16 MB)
- Disable WiFi when Thread is active
- Reduce Matter features (e.g., no OTA)
- Static config (no NVS storage)

---

## Appendix B: GPIO Pin Mapping

### ESP32-C6 DevKit Pin Assignments

| Function | ESP32-C6 Pin | RPi Pin | Notes |
|----------|-------------|---------|-------|
| Relay 1 | GPIO 0 | GPIO 23 | Active-LOW |
| Relay 2 | GPIO 1 | GPIO 24 | Active-LOW |
| Relay 3 | GPIO 2 | GPIO 25 | Active-LOW |
| 1-Wire Bus | GPIO 10 | GPIO 4 | Pull-up required (4.7kΩ) |
| VCC Control | GPIO 8 | GPIO 0 | Sensor power |
| Status LED | GPIO 15 | N/A | Built-in LED |
| UART TX | GPIO 16 | N/A | Debug logging |
| UART RX | GPIO 17 | N/A | Debug logging |

**Notes:**
- GPIO 0-5 are strapping pins (check datasheet for boot mode)
- Use GPIO 10+ for critical functions
- All GPIOs are 3.3V (use level shifter for 5V relays)

---

## Appendix C: Build System Example

### ESP32 Build Script

```rust
// build.rs
fn main() {
    #[cfg(feature = "esp32-c6")]
    {
        // ESP-IDF build integration
        embuild::espidf::sysenv::output();

        // Compile Matter SDK for ESP32
        let out_dir = std::env::var("OUT_DIR").unwrap();

        std::process::Command::new("cmake")
            .args(&[
                "-S", "third_party/connectedhomeip",
                "-B", &format!("{}/matter-build", out_dir),
                "-DBOARD=esp32c6",
                "-DENABLE_THREAD=1",
            ])
            .status()
            .expect("Failed to configure Matter SDK");

        std::process::Command::new("cmake")
            .args(&[
                "--build", &format!("{}/matter-build", out_dir),
                "--target", "chip-all",
            ])
            .status()
            .expect("Failed to build Matter SDK");

        // Link against Matter library
        println!("cargo:rustc-link-search=native={}/matter-build/lib", out_dir);
        println!("cargo:rustc-link-lib=static=chip");
    }

    #[cfg(feature = "rpi")]
    {
        // No special build steps for Raspberry Pi
        println!("cargo:rerun-if-changed=src/matter/ffi.rs");
    }
}
```

---

## Conclusion

Adapting thermos-rust to ESP32 RISC-V is a **significant but achievable undertaking**. The key challenges are:

1. **Async runtime migration** (tokio → embassy)
2. **GPIO abstraction** (rppal → esp-hal)
3. **1-Wire driver implementation** (Linux sysfs → RMT peripheral)
4. **Matter SDK integration** (ESP-IDF build system)
5. **Memory constraints** (512 KB SRAM)

With careful planning, phased implementation, and thorough testing, the project can be successfully ported while maintaining the core functionality. The HAL abstraction layer will allow both Raspberry Pi and ESP32 versions to coexist, maximizing code reuse.

**Recommended approach:**
- Start with **ESP32-C6** (Thread + WiFi for Matter)
- Use **Embassy** for async runtime
- Implement **RMT-based 1-Wire driver**
- Add **external PSRAM** if memory becomes an issue
- Follow the 8-month phased timeline

**Total estimated effort:** 850-1,350 hours (~6-8 months with 1-2 developers)

---

*This plan is a living document and should be updated as the project progresses and new challenges are discovered.*
