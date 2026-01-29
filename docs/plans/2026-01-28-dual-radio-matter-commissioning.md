# Dual-Radio Matter Commissioning Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable reliable Matter commissioning by using ESP32-C6 for WiFi and ESP32-H2 for BLE over UART HCI, eliminating single-radio time-sharing issues.

**Architecture:** The ESP32-C6 runs the rs-matter-embassy stack and handles WiFi. The ESP32-H2 acts as a BLE co-processor, exposing its controller over UART using HCI H4 protocol. We create a `DualRadioDriver` that implements `WifiCoexDriver` by using ESP-C6's WiFi and bt-hci's `SerialTransport`+`ExternalController` for BLE.

**Tech Stack:** rs-matter-embassy, bt-hci 0.6, esp-hal UART async, trouble-host Controller trait

---

## Prerequisites

Before starting implementation:

1. **Flash ESP32-H2 with HCI UART firmware:**
   
   Use the **NimBLE HCI example** which explicitly supports ESP32-H2:
   ```bash
   cd $IDF_PATH/examples/bluetooth/nimble/hci
   idf.py set-target esp32h2
   idf.py menuconfig
   ```
   
   Configure in menuconfig:
   - **Component config → Bluetooth → Controller Options → HCI Config:**
     - HCI mode: UART(H4)
     - HCI UART port: 1
     - HCI uart Hardware Flow ctrl: enabled
     - HCI uart Tx gpio: 1
     - HCI uart Rx gpio: 0
     - HCI uart RTS gpio: 3
     - HCI uart CTS gpio: 2
     - HCI uart baudrate: **115200** (not 921600 - see note below)
   
   ```bash
   idf.py build flash -p /dev/tty.usbserial-H2
   ```
   
   **Important:** The old `controller_hci_uart_esp32c3_and_esp32s3` example does NOT work
   for ESP32-H2 - it uses different APIs (`esp_bt_hci_tl_t`) that don't exist on H2.
   
   **Baud Rate Note:** Use 115200 baud, not 921600. At higher baud rates, the ESP32-C6's
   async UART read task isn't polled frequently enough during BLE commissioning, causing
   the hardware RX FIFO to overflow. Future improvement: use DMA-based UART.
   
   **Note:** ESP32-H2 has GPIOs 0-5, 8-14, 22-27. There is no GPIO6 or GPIO7.

2. **Wire C6 to H2:**
   | ESP32-C6   | Signal   | ESP32-H2   |
   |------------|----------|------------|
   | GPIO18 (TX)| UART TX  | GPIO0 (RX) |
   | GPIO19 (RX)| UART RX  | GPIO1 (TX) |
   | GND        | Ground   | GND        |
   
   **Optional hardware flow control:**
   | ESP32-C6   | Signal   | ESP32-H2   |
   |------------|----------|------------|
   | GPIO6 (RTS)| RTS      | GPIO2 (CTS)|
   | GPIO7 (CTS)| CTS      | GPIO3 (RTS)|

---

## Task 1: Add bt-hci Dependency and Feature Flag

**Files:**
- Modify: `thermos-esp32/Cargo.toml`

**Step 1: Add bt-hci as optional dependency**

In `thermos-esp32/Cargo.toml`, add after the `esp-hal-rmt-onewire` dependency:

```toml
# BLE HCI over UART for external controller (ESP32-H2)
bt-hci = { version = "0.6", optional = true }
```

**Step 2: Add external-ble feature flag**

In the `[features]` section, add:

```toml
# Enable external BLE controller over UART HCI (ESP32-H2 co-processor)
# Requires Matter feature. Use: cargo build -p thermos-esp32 --features "matter,external-ble"
external-ble = ["dep:bt-hci", "matter"]
```

**Step 3: Verify it compiles**

Run: `cargo check -p thermos-esp32 --features "matter,external-ble"`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add thermos-esp32/Cargo.toml
git commit -m "feat(matter): add bt-hci dependency for external BLE controller"
```

---

## Task 2: Add UART HCI Configuration Constants

**Files:**
- Modify: `thermos-esp32/src/config.rs`

**Step 1: Add HCI UART configuration constants**

Add before the `#[cfg(test)]` block at the end of `config.rs`:

```rust
// =============================================================================
// Dual-Radio BLE Configuration (ESP32-H2 over UART HCI)
// =============================================================================

/// HCI UART baud rate (must match ESP32-H2 firmware)
#[cfg(feature = "external-ble")]
pub const HCI_UART_BAUDRATE: u32 = 921600;

/// HCI UART TX pin (C6 -> H2)
/// Using GPIO18 to avoid strapping pins (4, 5, 8, 9, 15)
#[cfg(feature = "external-ble")]
pub const HCI_UART_TX_PIN: u8 = 18;

/// HCI UART RX pin (H2 -> C6)
/// Using GPIO19 to avoid strapping pins (4, 5, 8, 9, 15)
#[cfg(feature = "external-ble")]
pub const HCI_UART_RX_PIN: u8 = 19;

/// HCI UART RTS pin (hardware flow control)
#[cfg(feature = "external-ble")]
pub const HCI_UART_RTS_PIN: u8 = 6;

/// HCI UART CTS pin (hardware flow control)
#[cfg(feature = "external-ble")]
pub const HCI_UART_CTS_PIN: u8 = 7;
```

**Step 2: Verify it compiles**

Run: `cargo check -p thermos-esp32 --features "matter,external-ble"`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add thermos-esp32/src/config.rs
git commit -m "feat(matter): add HCI UART pin configuration for dual-radio"
```

---

## Task 3: Create DualRadioDriver Module

**Files:**
- Create: `thermos-esp32/src/dual_radio.rs`

**Step 1: Create the dual_radio module with driver implementation**

Create `thermos-esp32/src/dual_radio.rs`:

```rust
//! Dual-radio driver for ESP32-C6 (WiFi) + ESP32-H2 (BLE over UART HCI)
//!
//! This module provides `DualRadioDriver` which implements rs-matter-embassy's
//! `WifiCoexDriver` trait by:
//! - Using ESP32-C6's internal WiFi via esp-radio
//! - Communicating with ESP32-H2's BLE controller over UART HCI (H4 protocol)

use bt_hci::controller::ExternalController;
use bt_hci::transport::SerialTransport;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;

use esp_hal::uart::{UartRx, UartTx};
use esp_hal::Async;

use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::wifi::esp::EspWifiController;
use rs_matter_embassy::wireless::wifi::{
    BleDriver, BleDriverTask, WifiCoexDriver, WifiCoexDriverTask, WifiDriver, WifiDriverTask,
};
use rs_matter_embassy::wireless::SLOTS;

use log::info;

/// Dual-radio WiFi+BLE driver
///
/// Uses ESP32-C6's internal WiFi and ESP32-H2's BLE over UART HCI.
pub struct DualRadioDriver<'a, 'd> {
    controller: &'a esp_radio::Controller<'d>,
    wifi_peripheral: esp_hal::peripherals::WIFI<'d>,
    uart_rx: UartRx<'d, Async>,
    uart_tx: UartTx<'d, Async>,
}

impl<'a, 'd> DualRadioDriver<'a, 'd> {
    /// Create a new dual-radio driver
    ///
    /// # Arguments
    /// - `controller` - The esp-radio controller for WiFi
    /// - `wifi_peripheral` - The WiFi peripheral
    /// - `uart_rx` - UART RX for HCI from ESP32-H2
    /// - `uart_tx` - UART TX for HCI to ESP32-H2
    pub fn new(
        controller: &'a esp_radio::Controller<'d>,
        wifi_peripheral: esp_hal::peripherals::WIFI<'d>,
        uart_rx: UartRx<'d, Async>,
        uart_tx: UartTx<'d, Async>,
    ) -> Self {
        Self {
            controller,
            wifi_peripheral,
            uart_rx,
            uart_tx,
        }
    }
}

impl WifiDriver for DualRadioDriver<'_, '_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: WifiDriverTask,
    {
        let (mut controller, wifi_interface) = unwrap!(esp_radio::wifi::new(
            self.controller,
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::Config::default(),
        ));

        unwrap!(controller.set_power_saving(esp_radio::wifi::PowerSaveMode::None));

        task.run(
            wifi_interface.sta,
            EspWifiController::<NoopRawMutex>::new(controller),
        )
        .await
    }
}

impl WifiCoexDriver for DualRadioDriver<'_, '_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: WifiCoexDriverTask,
    {
        info!("DualRadioDriver: Initializing UART HCI transport to ESP32-H2");

        // Create BLE controller from UART transport
        let transport: SerialTransport<NoopRawMutex, _, _> =
            SerialTransport::new(&mut self.uart_rx, &mut self.uart_tx);
        let ble_ctl: ExternalController<_, SLOTS> = ExternalController::new(transport);

        info!("DualRadioDriver: BLE controller ready, initializing WiFi");

        // Initialize WiFi
        let (mut controller, wifi_interface) = unwrap!(esp_radio::wifi::new(
            self.controller,
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::Config::default(),
        ));

        unwrap!(controller.set_power_saving(esp_radio::wifi::PowerSaveMode::None));

        info!("DualRadioDriver: Running coex task with dual radios");

        task.run(
            wifi_interface.sta,
            EspWifiController::<NoopRawMutex>::new(controller),
            ble_ctl,
        )
        .await
    }
}

impl BleDriver for DualRadioDriver<'_, '_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: BleDriverTask,
    {
        info!("DualRadioDriver: Running BLE-only task via UART HCI");

        let transport: SerialTransport<NoopRawMutex, _, _> =
            SerialTransport::new(&mut self.uart_rx, &mut self.uart_tx);
        let ble_ctl: ExternalController<_, SLOTS> = ExternalController::new(transport);

        task.run(ble_ctl).await
    }
}
```

**Step 2: Register module in main.rs**

Add to `thermos-esp32/src/main.rs` after the other module declarations:

```rust
#[cfg(feature = "external-ble")]
mod dual_radio;
```

**Step 3: Verify it compiles**

Run: `cargo check -p thermos-esp32 --features "matter,external-ble"`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add thermos-esp32/src/dual_radio.rs thermos-esp32/src/main.rs
git commit -m "feat(matter): add DualRadioDriver for external BLE over UART"
```

---

## Task 4: Add Dual-Radio Matter Task

**Files:**
- Modify: `thermos-esp32/src/matter/stack.rs`

**Step 1: Add dual-radio matter task function**

Add at the end of `thermos-esp32/src/matter/stack.rs`, before the `#[cfg(test)]` block:

```rust
/// Run the Matter stack with dual-radio (C6 WiFi + H2 BLE)
///
/// This variant uses an external BLE controller over UART HCI instead of
/// the ESP32-C6's internal BLE, avoiding radio time-sharing issues.
#[cfg(feature = "external-ble")]
pub async fn matter_task_dual_radio<D, U>(driver: D, user_task: U)
where
    D: rs_matter_embassy::wireless::wifi::WifiCoexDriver
        + rs_matter_embassy::wireless::wifi::WifiDriver
        + rs_matter_embassy::wireless::BleDriver,
    U: rs_matter_embassy::stack::UserTask,
{
    info!("Matter: Starting dual-radio task (C6 WiFi + H2 BLE)...");

    let stack = mk_static!(EmbassyWifiMatterStack::<BUMP_SIZE, ()>).init_with(
        EmbassyWifiMatterStack::init(&DEV_DET, DEV_COMM, &TEST_DEV_ATT, epoch, esp_rand),
    );

    info!("Matter: Stack initialized (dual-radio mode)");
    info!(
        "Matter: Discriminator={}, Passcode={}",
        MATTER_DISCRIMINATOR, MATTER_PASSCODE
    );

    let thermostat = ZoneThermostatCluster::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    let identify = IdentifyHandler::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    let thermostat_ui = ThermostatUiConfigHandler::new(Dataver::new_rand(stack.matter().rand()));

    let handler = EmptyHandler
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_IDENTIFY)),
            Async(&identify),
        )
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(CLUSTER_THERMOSTAT_UI_CONFIG),
            ),
            Async(&thermostat_ui),
        )
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_THERMOSTAT)),
            Async(&thermostat),
        )
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(desc::DescHandler::CLUSTER.id),
            ),
            Async(desc::DescHandler::new(Dataver::new_rand(stack.matter().rand())).adapt()),
        );

    let persist = stack
        .create_persist_with_comm_window(DummyKvBlobStore)
        .await
        .unwrap();

    const NODE: Node = Node {
        id: 0,
        endpoints: &[
            EmbassyWifiMatterStack::<0, ()>::root_endpoint(),
            Endpoint {
                id: FIRST_ZONE_ENDPOINT,
                device_types: devices!(DEV_TYPE_THERMOSTAT),
                clusters: clusters!(
                    IDENTIFY_CLUSTER,
                    THERMOSTAT_UI_CONFIG_CLUSTER,
                    desc::DescHandler::CLUSTER,
                    THERMOSTAT_CLUSTER
                ),
            },
        ],
    };

    let matter = pin!(stack.run_coex(
        EmbassyWifi::new(driver, stack),
        &persist,
        (NODE, handler),
        user_task,
    ));

    info!("Matter: Running with dual-radio (C6 WiFi + H2 BLE)...");
    matter.await.unwrap();
}
```

**Step 2: Add required import at top of file**

Add to the existing imports if not present:

```rust
#[cfg(feature = "external-ble")]
use rs_matter_embassy::wireless::BleDriver;
```

**Step 3: Verify it compiles**

Run: `cargo check -p thermos-esp32 --features "matter,external-ble"`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add thermos-esp32/src/matter/stack.rs
git commit -m "feat(matter): add matter_task_dual_radio for external BLE"
```

---

## Task 5: Update main.rs for Dual-Radio Initialization

**Files:**
- Modify: `thermos-esp32/src/main.rs`

**Step 1: Add dual-radio imports**

Add to the imports section (after other cfg imports):

```rust
#[cfg(feature = "external-ble")]
use dual_radio::DualRadioDriver;
#[cfg(feature = "external-ble")]
use config::{HCI_UART_BAUDRATE, HCI_UART_TX_PIN, HCI_UART_RX_PIN};
#[cfg(feature = "external-ble")]
use esp_hal::uart::{Config as UartConfig, Uart};
#[cfg(feature = "external-ble")]
use matter::stack::matter_task_dual_radio;
```

**Step 2: Add dual-radio Matter task declaration**

Add after the existing `matter_main_task` function:

```rust
#[cfg(feature = "external-ble")]
#[embassy_executor::task]
async fn matter_main_task_dual_radio(
    controller: &'static EspRadioController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    uart_rx: esp_hal::uart::UartRx<'static, esp_hal::Async>,
    uart_tx: esp_hal::uart::UartTx<'static, esp_hal::Async>,
    user_task: (),
) {
    let driver = DualRadioDriver::new(controller, wifi, uart_rx, uart_tx);
    matter_task_dual_radio(driver, user_task).await
}
```

**Step 3: Add conditional initialization in main()**

In the `#[cfg(feature = "matter")]` block in `main()`, replace the existing Matter initialization with:

```rust
#[cfg(feature = "matter")]
{
    info!("------------------------------------");
    info!("Matter Configuration:");

    #[cfg(feature = "external-ble")]
    {
        info!("  Mode: Dual-radio (C6 WiFi + H2 BLE over UART)");
        info!(
            "  HCI UART: TX=GPIO{}, RX=GPIO{}, baud={}",
            HCI_UART_TX_PIN, HCI_UART_RX_PIN, HCI_UART_BAUDRATE
        );

        // Initialize esp-radio for WiFi only
        let controller =
            RADIO_CONTROLLER.init(esp_radio::init().expect("Failed to initialize esp-radio"));

        // Initialize HCI UART for ESP32-H2 communication
        let uart_config = UartConfig::default().with_baudrate(HCI_UART_BAUDRATE);
        let uart = Uart::new_async_with_config(
            peripherals.UART1,
            uart_config,
            peripherals.GPIO19, // RX
            peripherals.GPIO18, // TX
        )
        .expect("Failed to create HCI UART");

        let (uart_rx, uart_tx) = uart.split();

        // Spawn dual-radio Matter task
        spawner
            .spawn(matter_main_task_dual_radio(
                controller,
                peripherals.WIFI,
                uart_rx,
                uart_tx,
                (),
            ))
            .expect("Failed to spawn Matter task");
        info!("Matter task spawned (dual-radio mode)");
    }

    #[cfg(not(feature = "external-ble"))]
    {
        info!("  Mode: WiFi + BLE coexistence (single radio)");
        info!("  Zones: 1 (single zone test)");

        let controller =
            RADIO_CONTROLLER.init(esp_radio::init().expect("Failed to initialize esp-radio"));

        info!("------------------------------------");
        info!("MQTT Configuration (Matter mode):");
        info!("  Broker: {}:{}", MQTT_BROKER_HOST, MQTT_BROKER_PORT);
        info!("  Device ID: {}", MQTT_DEVICE_ID);
        info!(
            "  Auth: {}",
            if MQTT_USERNAME.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        let mqtt_config = MqttConfig::with_auth(
            MQTT_BROKER_HOST,
            MQTT_BROKER_PORT,
            MQTT_DEVICE_ID,
            MQTT_USERNAME,
            MQTT_PASSWORD,
        )
        .expect("Failed to create MQTT config");

        let _mqtt_bridge = matter::MqttBridge::new(mqtt_config, &MQTT_CHANNEL);

        spawner
            .spawn(matter_main_task(
                controller,
                peripherals.WIFI,
                peripherals.BT,
                (),
            ))
            .expect("Failed to spawn Matter task");
        info!("Matter task spawned (MQTT disabled - socket limit)");
    }

    info!("------------------------------------");
}
```

**Step 4: Verify it compiles**

Run: `cargo check -p thermos-esp32 --features "matter,external-ble"`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add thermos-esp32/src/main.rs
git commit -m "feat(matter): add dual-radio initialization in main"
```

---

## Task 6: Build and Flash Test

**Step 1: Build firmware with dual-radio feature**

Run: `cargo build -p thermos-esp32 --features "matter,external-ble" --release`
Expected: Build succeeds

**Step 2: Flash to ESP32-C6**

Run: `espflash flash --monitor target/riscv32imac-unknown-none-elf/release/thermos-esp32`
Expected: Firmware flashes and boots, showing "Mode: Dual-radio" in logs

**Step 3: Verify HCI communication**

Expected log output:
```
Matter Configuration:
  Mode: Dual-radio (C6 WiFi + H2 BLE over UART)
  HCI UART: TX=GPIO18, RX=GPIO19, baud=921600
...
DualRadioDriver: Initializing UART HCI transport to ESP32-H2
DualRadioDriver: BLE controller ready, initializing WiFi
```

**Step 4: Test Matter commissioning**

Use Home Assistant or chip-tool:
```bash
chip-tool pairing ble-wifi 1 "SSID" "password" 20202021 3840
```
Expected: Commissioning completes successfully

**Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix(matter): address issues found during dual-radio testing"
```

---

## Verification Checklist

- [ ] `cargo check -p thermos-esp32 --features "matter"` still compiles (single-radio)
- [ ] `cargo check -p thermos-esp32 --features "matter,external-ble"` compiles (dual-radio)
- [ ] `cargo test -p thermos-core` passes
- [ ] `cargo test -p thermos-matter` passes
- [ ] Firmware boots and shows dual-radio mode in logs
- [ ] BLE advertising visible on phone
- [ ] Matter commissioning completes with Home Assistant or HomeKit
