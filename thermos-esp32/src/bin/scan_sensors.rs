#![no_std]
#![no_main]

//! DS18B20 Sensor Scanner Utility
//!
//! This utility scans the 1-Wire bus and prints discovered DS18B20 sensor
//! addresses in a format ready to copy-paste into `config.rs`.
//!
//! # Usage
//!
//! Build and flash:
//! ```bash
//! cd thermos-esp32
//! WIFI_SSID="test" WIFI_PASSWORD="test" cargo build --release --bin scan_sensors
//! cargo espflash flash --release --bin scan_sensors
//! ```
//!
//! Then monitor the serial output to see discovered sensor addresses.
//!
//! # Note
//!
//! Hardware 1-Wire scanning requires `esp-hal-rmt-onewire` which has
//! compatibility issues with esp-hal 1.0. This utility provides the
//! structure and mock output for testing. When the dependency is updated,
//! enable real hardware scanning.

extern crate alloc;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use log::info;

// Import config for pin definitions
use thermos_esp32::config::ONEWIRE_PIN;
use thermos_esp32::sensors::SharedOneWireBus;

/// Main entry point for the sensor scanner
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    // Initialize ESP32 peripherals with default configuration
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize heap allocator (minimal for this utility)
    esp_alloc::heap_allocator!(size: 32768);

    // Initialize logging
    log::set_max_level(log::LevelFilter::Info);

    info!("========================================");
    info!("DS18B20 Sensor Scanner Utility");
    info!("========================================");
    info!("");
    info!("1-Wire bus GPIO: {}", ONEWIRE_PIN);
    info!("Scanning for DS18B20 temperature sensors...");
    info!("");

    // Small delay to ensure serial output is captured
    Timer::after(Duration::from_millis(500)).await;

    // Create the shared 1-Wire bus (placeholder for now)
    let bus = SharedOneWireBus::new_placeholder();

    // Attempt to scan for sensors
    match bus.scan().await {
        Ok(sensors) => {
            if sensors.is_empty() {
                info!("No DS18B20 sensors found on the bus.");
                info!("");
                info!("Note: Hardware 1-Wire support is pending.");
                info!("The esp-hal-rmt-onewire dependency needs updating for esp-hal 1.0.");
                info!("");
                info!("In the meantime, here's how to manually discover sensor addresses:");
                info!("1. Install Python ds18b20 library on a Raspberry Pi");
                info!("2. Connect sensors and run: python -c 'import glob; print(glob.glob(\"/sys/bus/w1/devices/28-*\"))'");
                info!("3. Convert the hex addresses to the Rust array format shown below");
                info!("");
            } else {
                info!("Found {} sensor(s):", sensors.len());
                info!("");

                for (i, address) in sensors.iter().enumerate() {
                    // Print in hex format
                    info!(
                        "Sensor #{}: {:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                        i + 1,
                        address[0],
                        address[1],
                        address[2],
                        address[3],
                        address[4],
                        address[5],
                        address[6],
                        address[7]
                    );

                    // Print in Rust array format for easy copy-paste
                    info!(
                        "  Rust:  [0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}, 0x{:02X}]",
                        address[0],
                        address[1],
                        address[2],
                        address[3],
                        address[4],
                        address[5],
                        address[6],
                        address[7]
                    );
                    info!("");
                }
            }
        }
        Err(e) => {
            info!("Error scanning bus: {}", e);
        }
    }

    // Print example output for documentation purposes
    info!("========================================");
    info!("Example sensor addresses (for reference):");
    info!("========================================");
    info!("");
    info!("A DS18B20 address is 8 bytes:");
    info!("  - Byte 0: Family code (always 0x28 for DS18B20)");
    info!("  - Bytes 1-6: Unique serial number");
    info!("  - Byte 7: CRC byte");
    info!("");
    info!("Example discovered sensor:");
    info!("  Hex:   28-AB-CD-EF-12-34-56-78");
    info!("  Rust:  [0x28, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78]");
    info!("");
    info!("Copy the Rust array format into config.rs ZONE_CONFIGS:");
    info!("");
    info!("  ZoneConfigData {{");
    info!("      name: \"Zone1\",");
    info!("      relay_pin: 5,");
    info!("      sensor_address: [0x28, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78],");
    info!("      default_target_c: 21.1,");
    info!("  }},");
    info!("");
    info!("========================================");
    info!("Scan complete.");
    info!("========================================");

    // Keep running to ensure output is flushed
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
