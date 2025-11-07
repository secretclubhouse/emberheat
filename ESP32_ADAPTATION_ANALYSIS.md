# ESP32 RISC-V Adaptation Analysis - Current Thermos-Rust State

## CURRENT PLATFORM: LINUX/RASPBERRY PI (NOT ESP32)

### Key Finding
**This project currently targets Linux systems (Raspberry Pi recommended) - NOT ESP32 microcontrollers.**

The git branch `claude/esp32-riscv-adaptation-plan` indicates planning for a future ESP32 adaptation, but no ESP32-specific code exists in the current codebase.

---

## 1. CURRENTLY TARGETED ARCHITECTURE

### Operating System
- **Primary Target:** Linux (Raspberry Pi recommended)
- **Supported Targets:** 
  - `armv7-unknown-linux-gnueabihf` (Raspberry Pi 2/3/4, 32-bit)
  - `aarch64-unknown-linux-gnu` (Raspberry Pi 3/4, 64-bit)
  - `x86_64-unknown-linux-gnu` (x86 systems, for development)

### Hardware Platform
- **GPIO Library:** `rppal` (Raspberry Pi Peripheral Access Library)
- **1-Wire Sensors:** Linux `/sys/bus/w1/` filesystem interface
- **System Requirements:** Linux kernel with GPIO and 1-Wire support

### Key Platform Assumptions in Code
1. **GPIO Access via rppal:**
   - File: `/home/user/thermos-rust/src/gpio/mod.rs`
   - Uses `rppal::gpio::{Gpio, Level, OutputPin}`
   - Active-LOW relay logic
   - No abstraction layer for different GPIO implementations

2. **1-Wire via Linux sysfs:**
   - File: `/home/user/thermos-rust/src/sensors/onewire.rs`
   - Reads from `/sys/bus/w1/devices/28-*/w1_slave`
   - Hardcoded Linux filesystem path: `const ONEWIRE_BASE_PATH: &str = "/sys/bus/w1/devices"`
   - Parses kernel-provided temperature data

3. **Configuration via Linux filesystem:**
   - File: `/home/user/thermos-rust/src/config/mod.rs`
   - Searches: `/etc/thermos-rust/config.json`, `~/.config/thermos-rust/config.json`, `./config/config.json`
   - Home directory expansion using `$HOME` environment variable

4. **Redis for state persistence:**
   - File: `/home/user/thermos-rust/src/persistence/mod.rs`
   - Network-based persistent storage
   - Default: `redis://localhost:6379`

5. **Prometheus metrics export:**
   - File: `/home/user/thermos-rust/src/metrics/mod.rs`
   - HTTP server on port 8080
   - Requires tokio async runtime

---

## 2. ARCHITECTURE-SPECIFIC CODE ANALYSIS

### Complete File Listing
```
src/
├── main.rs                    # Linux/async focused
├── config/mod.rs              # Linux filesystem config loading
├── devices/mod.rs             # Core logic (platform-independent)
├── gpio/mod.rs                # rppal-dependent GPIO abstraction
├── sensors/
│   ├── mod.rs                 # Sensor traits (platform-independent)
│   └── onewire.rs             # Linux sysfs 1-Wire implementation
├── persistence/mod.rs         # Redis (platform-independent)
├── metrics/mod.rs             # Prometheus HTTP (platform-independent)
├── matter/                     # Matter protocol (platform-independent)
│   ├── mod.rs                 # Matter device logic
│   └── ffi/                   # C++ wrapper for Connected Home IP
└── mocks/                      # Test mocking (platform-independent)
```

### Platform-Dependent Code Summary

#### 1. GPIO Module (CRITICAL DEPENDENCY)
**File:** `/home/user/thermos-rust/src/gpio/mod.rs` (372 lines)

**Direct rppal Dependencies:**
```rust
use rppal::gpio::{Gpio, Level, OutputPin};

pub struct GpioController {
    gpio: Gpio,  // rppal GPIO instance - PLATFORM-SPECIFIC
    relays: Arc<Mutex<HashMap<u8, RelayController>>>,
    vcc_controller: Arc<Mutex<Option<VccController>>>,
}

pub struct RelayController {
    output_pin: OutputPin,  // rppal OutputPin - PLATFORM-SPECIFIC
    pin_number: u8,
}

pub struct VccController {
    pin: OutputPin,  // rppal OutputPin - PLATFORM-SPECIFIC
}
```

**Hard Requirements:**
- `Gpio::new()` initializes the only rppal GPIO instance
- `gpio.get(pin_number)?.into_output()` creates platform-specific output pins
- Pin numbering assumes BCM (Broadcom) GPIO numbering
- No abstraction layer exists - direct rppal types used throughout

#### 2. Sensor Module (1-Wire Implementation)
**File:** `/home/user/thermos-rust/src/sensors/onewire.rs` (222 lines)

**Linux-Specific Code:**
```rust
// Hardcoded Linux sysfs path
const ONEWIRE_BASE_PATH: &str = "/sys/bus/w1/devices";

// Device path construction
let device_path = PathBuf::from(ONEWIRE_BASE_PATH)
    .join(&normalized_id)
    .join("w1_slave");

// Async file I/O reading kernel-provided data
let content = timeout(SENSOR_READ_TIMEOUT, fs::read_to_string(&self.device_path))
    .await
    .map_err(|_| SensorError::Timeout)?
    .map_err(|e| SensorError::IoError(e.to_string()))?;

// Parsing kernel w1_slave format:
// "5e 01 4b 46 7f ff 0c 10 1c : crc=1c YES\n5e 01 4b 46 7f ff 0c 10 1c t=21875\n"
```

**Hard Requirements:**
- Linux 1-Wire kernel module (`w1_gpio`, `w1_therm`)
- `/sys/bus/w1/devices/` filesystem interface
- Kernel manages 1-Wire communication and data parsing
- DS18B20 sensors identified by "28-*" directory names

#### 3. Configuration Module
**File:** `/home/user/thermos-rust/src/config/mod.rs` (470 lines)

**Linux-Specific Patterns:**
```rust
// Linux filesystem paths
const CONFIG_SEARCH_PATHS: &[&str] = &[
    "/etc/thermos-rust/config.json",
    "~/.config/thermos-rust/config.json",
    "./config/config.json",
];

// Home directory expansion
if let Some(stripped) = path_str.strip_prefix("~/") {
    if let Some(home) = std::env::var_os("HOME") {  // Unix HOME variable
        PathBuf::from(home).join(stripped)
    }
}
```

**Hard Requirements:**
- Unix filesystem conventions
- `HOME` environment variable for user config
- `/etc` directory for system-wide config
- `~/.config/` for user-specific config

#### 4. Main Entrypoint
**File:** `/home/user/thermos-rust/src/main.rs` (237 lines)

**Linux Assumptions:**
```rust
// Signal handling via Unix signals
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;

let signals = Signals::new([SIGTERM, SIGINT])?;  // Unix signals
```

**Hardcoded Linux-specific paths:**
```rust
const CONFIG_SEARCH_PATHS: &[&str] = &[
    "/etc/thermos-rust/config.json",
    "~/.config/thermos-rust/config.json",
    "./config/config.json",
];
```

---

## 3. HARDWARE PERIPHERALS & ABSTRACTION

### Supported Peripherals

#### 1. GPIO-Based Relay Control
- **Purpose:** Heating on/off control
- **Logic:** Active-LOW (LOW=heat ON, HIGH=heat OFF)
- **Pins:** Configurable per-zone (typ. GPIO 23, 24, 25)
- **Driver:** rppal GPIO library
- **Abstraction:** `GpioController` + `RelayController` (THIN - no flexibility)

#### 2. 1-Wire DS18B20 Temperature Sensors
- **Purpose:** Primary temperature reading
- **Protocol:** 1-Wire (Dallas/Maxim)
- **Bus Pin:** Configurable per-zone (can be shared, typ. GPIO 4)
- **Driver:** Linux kernel w1_gpio + w1_therm modules
- **Data Interface:** `/sys/bus/w1/devices/*/w1_slave` files
- **Abstraction:** `OneWireSensor` + `TemperatureSensor` trait (GOOD - trait-based)

#### 3. VCC Power Control
- **Purpose:** Sensor power cycling on error
- **Logic:** GPIO pin to control VCC supply
- **Pin:** Fixed GPIO 0 (hardcoded!)
- **Driver:** rppal GPIO library
- **Abstraction:** `VccController` (used by `GpioController`)

#### 4. HTTP Sensors (Fallback)
- **Purpose:** Supplementary/fallback temperature readings
- **Protocol:** HTTP REST with JSON response: `{"temp_c": 20.5}`
- **Driver:** `reqwest` HTTP client
- **Abstraction:** `HttpSensor` + `TemperatureSensor` trait (GOOD)

#### 5. Redis Client
- **Purpose:** State persistence
- **Network-based:** No hardware dependency
- **Driver:** `redis-rs` client library
- **Abstraction:** `StateManager` (GOOD - completely platform-independent)

#### 6. Prometheus Metrics HTTP Server
- **Purpose:** Metrics export
- **Network-based:** No hardware dependency
- **Port:** 8080 (configurable in main.rs)
- **Driver:** `prometheus` crate + tokio HTTP server
- **Abstraction:** `MetricsServer` (GOOD - completely platform-independent)

### Abstraction Quality Assessment

| Component | Abstraction Level | Platform-Dependent | Comments |
|-----------|-------------------|-------------------|----------|
| GPIO | THIN | **YES (rppal)** | Hardcoded rppal types, no trait abstraction |
| 1-Wire | GOOD | **YES (Linux sysfs)** | Trait-based but single Linux impl |
| HTTP Sensors | GOOD | NO | Generic HTTP client, platform-independent |
| Redis | EXCELLENT | NO | Network-based, fully platform-independent |
| Metrics | EXCELLENT | NO | HTTP server, fully platform-independent |
| Control Logic | EXCELLENT | NO | Pure Rust, platform-independent |
| Configuration | MODERATE | **YES (Unix)** | Unix paths, HOME variable, /etc conventions |

---

## 4. CONDITIONAL COMPILATION & FEATURE FLAGS

### Current Features in Cargo.toml
```toml
[features]
# Optional feature to regenerate FFI bindings
# bindgen = ["dep:bindgen"]
default = []

[target.'cfg(target_os = "linux")'.dependencies]
rppal = "0.22"  # Raspberry Pi GPIO library (Linux only)
```

### Conditional Compilation Usage

#### Optional bindgen Feature
```rust
#[cfg(feature = "bindgen")]
{
    let bindings = bindgen::Builder::default()
        .header("src/matter/ffi/matter_wrapper.h")
        // ... generate FFI bindings
}
```

#### Matter SDK Availability (build.rs)
```rust
if chip_available {
    build.define("MATTER_SDK_AVAILABLE", "1");
    println!("cargo:rustc-cfg=matter_sdk_available");
} else {
    build.define("MATTER_SDK_STUB", "1");
    println!("cargo:rustc-cfg=matter_sdk_stub");
}

#[cfg(feature = "matter_sdk_available")]
{
    // Full Matter implementation
}

#[cfg(feature = "matter_sdk_stub")]
{
    // Stub implementation
}
```

### BUILD-TIME Configurations (build.rs)
```rust
// Platform detection
fn get_platform_string() -> String {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
    format!("{}-{}", os, arch_normalized)
}

// C++ compiler flags (platform-dependent)
let mut build = cc::Build::new();
build
    .cpp(true)
    .file("src/matter/ffi/matter_wrapper.cpp")
    .flag_if_supported("-std=c++17")
    .flag_if_supported("-Wall");
```

### **NO EXISTING CONDITIONAL CODE FOR DIFFERENT HAL IMPLEMENTATIONS**
There are **no `#[cfg(target_os = "...")]` guards** around GPIO or sensor code, meaning:
- The code **assumes Linux/rppal availability at compile time**
- Cannot compile for ESP32 without removing/replacing GPIO module
- No abstraction layer for swapping hardware implementations

---

## 5. PROJECT STRUCTURE & DEPENDENCIES

### Module Dependency Graph
```
main.rs
├── config/ (configuration loading)
├── devices/ (thermostat logic)
│   ├── gpio/ (GPIO control - rppal)
│   ├── sensors/ (temperature reading)
│   │   ├── onewire.rs (Linux 1-Wire)
│   │   └── HttpSensor
│   ├── persistence/ (Redis)
│   └── metrics/ (Prometheus)
├── matter/ (Matter protocol)
├── persistence/ (Redis client)
└── metrics/ (Prometheus server)
```

### Direct External Dependencies

**Async Runtime:**
- `tokio` v1.48 - Async runtime (NOT compatible with bare-metal)
- `futures` 0.3 - Futures utilities
- `async-trait` 0.1 - Async trait support

**Hardware Access:**
- `rppal` 0.22 - Raspberry Pi GPIO (**Linux-only, under `target_os = "linux"` condition**)

**Networking:**
- `reqwest` 0.12 - HTTP client (for HTTP sensors)
- `redis` 0.27 - Redis client (network-based, works everywhere)

**Configuration:**
- `serde` 1.0 + `serde_json` 1.0 - JSON parsing
- `toml` 0.8 - TOML support

**Monitoring:**
- `prometheus` 0.14 - Metrics collection
- `tracing` 0.1 - Structured logging

**Matter Protocol:**
- Custom FFI wrapper to Connected Home IP C++ library (third_party/connectedhomeip git submodule)

**System Integration:**
- `signal-hook` 0.3 - Unix signal handling
- `signal-hook-tokio` 0.3 - Tokio signal integration

### Build Dependencies
```toml
[build-dependencies]
cc = "1.0"  # C/C++ compiler wrapper for building matter_wrapper.cpp
# bindgen = { version = "0.69", optional = true }  # For auto-generating FFI bindings
```

### Test Dependencies
- `mockall` 0.13 - Mocking framework
- `tokio-test` 0.4 - Tokio testing utilities
- `tempfile` 3.14 - Temporary file handling
- `pretty_assertions` 1.4 - Better assertion output
- `serial_test` 3.2 - Serial test execution

### Dependency Impact on ESP32 Adaptation

| Dependency | ESP32 Compatibility | Impact |
|------------|-------------------|--------|
| tokio | NO (no_std) | Requires async runtime replacement |
| rppal | NO (Linux-only) | Must replace with esp-idf-hal |
| reqwest | MAYBE (with features) | HTTP client available, may need adaptation |
| redis | MAYBE (network) | Works if ESP32 connected to network |
| serde/serde_json | YES | Platform-independent |
| prometheus | MAYBE (needs http) | HTTP server available, needs adaptation |
| signal-hook | NO (Unix signals) | Not applicable for microcontroller |
| Matter FFI | MAYBE | Requires ESP32 build of Connected Home IP |

---

## 6. KEY FILES AND LINE COUNTS

### Source Code Size
```
src/main.rs                           237 lines
src/config/mod.rs                     470 lines
src/devices/mod.rs                    581 lines
src/gpio/mod.rs                       372 lines (PLATFORM-DEPENDENT)
src/sensors/mod.rs                    318 lines
src/sensors/onewire.rs                222 lines (PLATFORM-DEPENDENT)
src/persistence/mod.rs                333 lines
src/metrics/mod.rs                    279 lines
src/matter/mod.rs                     928 lines
src/matter/ffi/mod.rs                 [FFI bindings]
src/mocks/                            ~700 lines (testing only)

Total Platform-Dependent Code:        ~1,200 lines (GPIO + 1-Wire + Config + Signals)
Total Platform-Independent Code:      ~2,500 lines (Core Logic + Matter + Persistence)
```

### Documentation
- README.md: 670 lines
- ARCHITECTURE.md: 380 lines
- PROJECT_SUMMARY.md: 300+ lines
- TESTING.md: 350+ lines
- QUICK_START.md: 303 lines
- MATTER_SETUP.md: 600+ lines
- GITHUB_ACTIONS_SETUP.md: 400+ lines

### Configuration
```
Cargo.toml              - Dependency and build config
build.rs                - Build script (184 lines) for Matter C++ compilation
config/config.example.json - Example configuration (JSON)
docker-compose.yml      - Multi-service Docker Compose stack
Dockerfile              - Standard x86/ARM Docker image
Dockerfile.armv7        - ARMv7-specific optimization
Makefile                - Build automation (286 lines)
```

### Test Coverage
- `tests/integration_test.rs` - Integration tests with hardware
- `tests/mock_integration_tests.rs` - Cross-platform mock tests
- Unit tests in source files (50+ total tests)
- Mock implementations for testing (gpio_mock.rs, sensor_mock.rs, state_mock.rs)

---

## 7. MATTER PROTOCOL INTEGRATION

### FFI Architecture
**Files:**
- `/home/user/thermos-rust/src/matter/ffi/mod.rs`
- `/home/user/thermos-rust/src/matter/ffi/wrapper.rs`
- `/home/user/thermos-rust/src/matter/ffi/bindings.rs` (auto-generated)
- `/home/user/thermos-rust/src/matter/ffi/matter_wrapper.cpp`

**Build Process:**
1. Compiles `matter_wrapper.cpp` using cc crate (C++ compiler wrapper)
2. Links against Connected Home IP library (from third_party/connectedhomeip submodule)
3. Optionally generates Rust bindings with bindgen
4. Uses conditional compilation for Matter SDK availability

**Platform Implications:**
- Connected Home IP library must be built for ESP32 target
- Requires architecture-specific prebuilt libraries or source build
- C++ compiler flags may differ for ESP32 (RISC-V)

---

## SUMMARY: ESP32 ADAPTATION CHALLENGES

### What MUST Be Replaced

1. **GPIO Implementation** (HIGH PRIORITY)
   - Current: rppal (Raspberry Pi specific)
   - Needed: esp-idf-hal or esp-hal (community-maintained)
   - Effort: Significant (trait abstraction needed)
   - Files affected: `src/gpio/mod.rs` completely, `src/devices/mod.rs` partially

2. **1-Wire Implementation** (HIGH PRIORITY)
   - Current: Linux `/sys/bus/w1/` kernel interface
   - Needed: ESP-IDF 1-Wire driver or bit-banging implementation
   - Effort: Significant (driver integration or low-level protocol)
   - Files affected: `src/sensors/onewire.rs` completely

3. **Signal Handling** (MEDIUM PRIORITY)
   - Current: Unix signals (SIGTERM, SIGINT)
   - Needed: Button press, RTC wakeup, or web-based shutdown
   - Effort: Moderate
   - Files affected: `src/main.rs` partially

4. **Tokio Runtime** (MEDIUM PRIORITY)
   - Current: Tokio async runtime (requires std)
   - Options: 
     - Keep tokio (embedded-hal has some support)
     - Switch to embassy-rs (embedded async)
     - Use bare threads (simple but not async)
   - Effort: Moderate to High depending on choice
   - Files affected: `src/main.rs`, all async code

5. **Configuration Loading** (LOW PRIORITY)
   - Current: Linux filesystem paths
   - Needed: SPIFFS/LittleFS on ESP32 or HTTP provisioning
   - Effort: Low to Moderate
   - Files affected: `src/config/mod.rs`

### What CAN Stay As-Is

1. **Device/Control Logic** - Pure Rust, platform-independent
2. **Sensor Trait System** - Abstraction allows new implementations
3. **HTTP Sensor Support** - Works on any network-enabled platform
4. **Redis Persistence** - Works on any network-enabled platform
5. **Metrics Export** - Can work with embedded HTTP server
6. **Matter Protocol** - Just needs Connected Home IP built for ESP32

### Estimated Effort

| Component | Effort | Lines to Change |
|-----------|--------|-----------------|
| GPIO abstraction | 200-300 hours | 400-600 |
| 1-Wire implementation | 150-200 hours | 300-400 |
| Async runtime (tokio->embassy) | 200-400 hours | 500-1000 |
| Configuration system | 50-100 hours | 100-200 |
| Matter SDK porting | 100-200 hours | (C++ build config) |
| Testing & integration | 100-150 hours | 500+ |
| Documentation | 50-100 hours | 500+ |
| **TOTAL** | **850-1350 hours** | **2200-3600 lines** |

---

## CONCLUSION

**Thermos-Rust is currently a fully functional Raspberry Pi/Linux thermostat system with NO ESP32 code present.** While the core control logic is platform-independent, significant architectural changes would be needed to support ESP32:

1. **Trait-based abstraction needed** for GPIO and sensors (currently hardcoded to Linux/rppal)
2. **Embedded async runtime** needed instead of tokio (embassy-rs recommended)
3. **1-Wire driver integration** needed (ESP-IDF or bit-banging)
4. **Configuration system** needs adaptation for embedded storage (SPIFFS/LittleFS)
5. **Matter SDK** needs ESP32 build variant

The branch name `claude/esp32-riscv-adaptation-plan` suggests this is a planning branch for a future adaptation - the groundwork would involve creating proper hardware abstraction layers before adding ESP32 support.
