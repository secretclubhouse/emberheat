#![no_std]
#![no_main]

//! DS18B20 Sensor Scanner Utility
//!
//! This utility scans the 1-Wire bus and prints discovered DS18B20 sensor
//! addresses in a format ready to copy-paste into `config.rs`.
//!
//! Uses the RMT peripheral for precise 1-Wire timing.
//!
//! # Usage
//!
//! Build and flash:
//! ```bash
//! cd thermos-esp32
//! WIFI_SSID="test" WIFI_PASSWORD="test" cargo build --release --bin scan_sensors
//! espflash flash --monitor target/riscv32imac-unknown-none-elf/release/scan_sensors
//! ```
//!
//! Then monitor the serial output to see discovered sensor addresses.

extern crate alloc;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rmt::Rmt;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use log::info;

// ESP-IDF bootloader app descriptor - required for espflash
esp_bootloader_esp_idf::esp_app_desc!();

use thermos_esp32::config::ONEWIRE_PIN;
use thermos_esp32::onewire_rmt::{Address, OneWire, Search, SearchError};

/// Main entry point for the sensor scanner
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    // Initialize logging
    esp_println::logger::init_logger_from_env();

    // Initialize ESP32 peripherals with default configuration
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize heap allocator (minimal for this utility)
    esp_alloc::heap_allocator!(size: 32768);

    // Initialize esp-rtos scheduler
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("========================================");
    println!("DS18B20 Sensor Scanner Utility");
    println!("========================================");
    info!("========================================");
    info!("DS18B20 Sensor Scanner Utility (RMT-based)");
    info!("========================================");
    info!("");
    info!("1-Wire bus GPIO: {}", ONEWIRE_PIN);
    info!("Scanning for DS18B20 temperature sensors...");
    info!("");

    // Small delay to ensure serial output is captured
    Timer::after(Duration::from_millis(500)).await;

    // Initialize RMT peripheral for 1-Wire communication (in async mode)
    let rmt = Rmt::new(peripherals.RMT, esp_hal::time::Rate::from_mhz(80))
        .unwrap()
        .into_async();

    // Create the 1-Wire bus on GPIO21 using RMT channels
    let mut onewire = match OneWire::new(rmt.channel0, rmt.channel2, peripherals.GPIO21) {
        Ok(ow) => ow,
        Err(e) => {
            info!("Failed to initialize 1-Wire bus: {:?}", e);
            loop {
                Timer::after(Duration::from_secs(60)).await;
            }
        }
    };

    // First, try to detect if any device is present
    info!("Sending reset pulse...");
    match onewire.reset().await {
        Ok(false) => {
            info!("No device responded to reset pulse.");
            info!("");
            info!("Troubleshooting tips:");
            info!("  1. Check wiring: DQ to GPIO21, VCC to 3.3V, GND to GND");
            info!("  2. Ensure 4.7kΩ pull-up resistor between DQ and VCC");
            info!("  3. Check power supply is adequate");
            info!("  4. Try shorter wires or a single sensor first");
        }
        Err(e) => {
            info!("Error during reset: {:?}", e);
        }
        Ok(true) => {
            info!("Device presence detected!");
            info!("");

            // Always use search algorithm - it works correctly with any number of devices
            // (READ ROM command only works with exactly one device on the bus)
            info!("Searching for all DS18B20 devices on bus...");
            info!("");

            let mut search = Search::new();
            let mut sensors: heapless::Vec<Address, 10> = heapless::Vec::new();

            loop {
                // Small delay between search iterations to let bus settle
                Timer::after(Duration::from_millis(10)).await;

                match search.next(&mut onewire).await {
                    Ok(address) => {
                        if address.is_ds18b20() {
                            info!("Found DS18B20: {}", address.to_id_string());
                            let _ = sensors.push(address);
                        } else {
                            info!(
                                "Found non-DS18B20 device (family 0x{:02X}): {}",
                                address.family_code(),
                                address
                            );
                        }
                    }
                    Err(SearchError::SearchComplete) => {
                        break;
                    }
                    Err(SearchError::NoDevicesPresent) => {
                        info!("No devices responded during search.");
                        break;
                    }
                    Err(SearchError::BusError(e)) => {
                        info!("Bus error during search: {:?}", e);
                        break;
                    }
                }

                // Safety limit to prevent infinite loops
                if sensors.len() >= 10 {
                    info!("Reached sensor limit (10), stopping search.");
                    break;
                }
            }

            info!("");

            if sensors.is_empty() {
                info!("No DS18B20 sensors found.");
                info!("");
                info!("Troubleshooting tips:");
                info!("  1. Check wiring: DQ to GPIO21, VCC to 3.3V, GND to GND");
                info!("  2. Ensure 4.7kΩ pull-up resistor between DQ and VCC");
                info!("  3. DS18B20 family code is 0x28 - other 1-Wire devices won't be listed");
            } else {
                info!("========================================");
                info!("Found {} DS18B20 sensor(s)", sensors.len());
                info!("========================================");
                info!("");

                for (i, address) in sensors.iter().enumerate() {
                    info!("Sensor #{}:", i + 1);
                    info!("  Address: {}", address);
                    info!("  ID:      {}", address.to_id_string());

                    // Try to read temperature using MATCH ROM (works with multiple devices)
                    match onewire.read_temperature(*address).await {
                        Ok(temp) => {
                            info!(
                                "  Temperature: {:.2}°C ({:.2}°F)",
                                temp,
                                temp * 9.0 / 5.0 + 32.0
                            );
                        }
                        Err(e) => {
                            info!("  Failed to read temperature: {:?}", e);
                        }
                    }
                    info!("");
                }

                // Print ready-to-use config
                info!("========================================");
                info!("Copy-paste config for config.rs:");
                info!("========================================");
                info!("");

                for (i, address) in sensors.iter().enumerate() {
                    let bytes = address.0.to_le_bytes();
                    info!("// Zone {} sensor", i + 1);
                    info!("sensor_id: \"{}\",", address.to_id_string());
                    info!(
                        "// Raw bytes: [0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}]",
                        bytes[0], bytes[1], bytes[2], bytes[3],
                        bytes[4], bytes[5], bytes[6], bytes[7]
                    );
                    info!("");
                }
            }
        }
    }

    info!("========================================");
    info!("Scan complete.");
    info!("========================================");

    // Keep running to ensure output is flushed
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
