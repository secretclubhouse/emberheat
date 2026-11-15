//! Multi-Zone Thermostat Configuration
//!
//! This module defines the compile-time configuration for the thermostat.
//! The number of zones is configurable via `NUM_ZONES` (1-16 supported).
//!
//! # Configuring Zone Count
//!
//! To change the number of zones:
//! 1. Set `NUM_ZONES` to desired value (1-16)
//! 2. Ensure `ZONE_CONFIGS` has exactly `NUM_ZONES` entries
//! 3. Rebuild the firmware
//!
//! **Note:** Adding zones beyond 7 requires adding corresponding `zoneN_task`
//! functions and static cells in `main.rs` (Embassy requires concrete task functions).
//!
//! # Example: 3-Zone Configuration
//!
//! ```ignore
//! pub const NUM_ZONES: usize = 3;
//!
//! pub const ZONE_CONFIGS: [ZoneConfigData; NUM_ZONES] = [
//!     ZoneConfigData { name: "Zone1", relay_pin: 0, sensor_address: [0x28, ...], default_target_c: 21.1 },
//!     ZoneConfigData { name: "Zone2", relay_pin: 1, sensor_address: [0x28, ...], default_target_c: 21.1 },
//!     ZoneConfigData { name: "Zone3", relay_pin: 2, sensor_address: [0x28, ...], default_target_c: 21.1 },
//! ];
//! ```
//!
//! # Sensor Addresses
//!
//! The `sensor_address` fields in `ZONE_CONFIGS` contain placeholder values.
//! To get actual sensor addresses:
//!
//! 1. Connect DS18B20 sensors one at a time
//! 2. Run the sensor scan utility to discover ROM addresses
//! 3. Update the addresses in this file
//!
//! Each DS18B20 has a unique 64-bit ROM address starting with family code 0x28.

/// GPIO pin for the shared 1-Wire bus (all DS18B20 sensors)
/// Note: Avoid strapping pins (4, 5, 8, 9, 15) and USB pins (12, 13, 16, 17)
pub const ONEWIRE_PIN: u8 = 21;

// ============================================================================
// Flash Persistence Configuration
// ============================================================================

/// Start address of persistence partition in flash
/// Must not overlap with app or other partitions
pub const PERSIST_PARTITION_START: u32 = 0x310000;

/// Size of persistence partition (64KB)
/// Provides ~25x headroom for sequential-storage wear leveling
pub const PERSIST_PARTITION_SIZE: u32 = 0x10000;

/// Flash range for persistence partition
pub const PERSIST_FLASH_RANGE: core::ops::Range<u32> =
    PERSIST_PARTITION_START..(PERSIST_PARTITION_START + PERSIST_PARTITION_SIZE);

/// Interval between persistence checks (seconds)
pub const PERSIST_CHECK_INTERVAL_SECS: u64 = 5;

// ============================================================================
// Factory Reset Configuration
// ============================================================================

/// GPIO pin for factory reset button (directly to GND when pressed)
/// Hold during boot to trigger factory reset
pub const FACTORY_RESET_PIN: u8 = 9;

/// How long reset button must be held to confirm factory reset (seconds)
pub const FACTORY_RESET_HOLD_SECS: u64 = 5;

// ============================================================================
// Crash Fail-Safe Configuration (see failsafe.rs)
// ============================================================================

/// A zone control loop that has not run for this long (seconds) is treated as
/// hung: relays are forced OFF and the controller restarts. Must comfortably
/// exceed the control interval plus the longest executor stall (flash erase,
/// Matter commissioning crypto).
pub const ZONE_STALL_TIMEOUT_SECS: u64 = 20;

/// Hardware watchdog timeout (seconds). If the supervisor task itself stops
/// running, the chip resets this long after its last feed.
pub const WATCHDOG_TIMEOUT_SECS: u64 = 20;

/// Pause between a panic and the restart (milliseconds). Rate-limits restart
/// loops when a panic repeats at boot; relays are already OFF while it runs.
pub const PANIC_RESTART_DELAY_MS: u32 = 5000;

/// Number of heating zones supported
/// TEMP: Reduced to 1 for Matter commissioning testing
pub const NUM_ZONES: usize = 1;

/// Compile-time zone configuration data
///
/// This struct is designed for static initialization and can be
/// converted to runtime `ZoneConfig` as needed.
#[derive(Debug, Clone, Copy)]
pub struct ZoneConfigData {
    /// Human-readable zone name
    pub name: &'static str,

    /// GPIO pin number for relay control
    pub relay_pin: u8,

    /// DS18B20 64-bit ROM address
    ///
    /// NOTE: These are placeholder addresses. Run sensor scan utility
    /// to discover actual addresses for your sensors.
    pub sensor_address: [u8; 8],

    /// Default target temperature in Celsius
    pub default_target_c: f32,
}

/// Configuration for all 7 heating zones
///
/// # Placeholder Addresses
///
/// The sensor addresses are placeholders (0x28, 0, 0, 0, 0, 0, 0, N).
/// You must update these with actual DS18B20 ROM addresses discovered
/// by running the sensor scan utility.
///
/// # GPIO Assignments
///
/// Safe GPIO pins on ESP32-C6 (avoiding USB-UART on 16/17, USB on 12/13,
/// and strapping pins 4, 5, 8, 9, 15):
///
/// - Zone1: GPIO 0
/// - Zone2: GPIO 1
/// - Zone3: GPIO 2
/// - Zone4: GPIO 3
/// - Zone5: GPIO 10
/// - Zone6: GPIO 11
/// - Zone7: GPIO 22
///
/// Note: GPIO 4 is reserved for 1-Wire bus (ONEWIRE_PIN)
// TEMP: Reduced to 1 zone for Matter commissioning testing
// Original 7-zone config commented out below
pub const ZONE_CONFIGS: [ZoneConfigData; NUM_ZONES] = [ZoneConfigData {
    name: "Zone1",
    relay_pin: 0,
    sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 1],
    default_target_c: 21.1,
}];

/*
// Original 7-zone configuration:
pub const ZONE_CONFIGS_FULL: [ZoneConfigData; 7] = [
    ZoneConfigData {
        name: "Zone1",
        relay_pin: 0,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 1],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone2",
        relay_pin: 1,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 2],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone3",
        relay_pin: 2,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 3],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone4",
        relay_pin: 3,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 4],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone5",
        relay_pin: 10,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 5],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone6",
        relay_pin: 11,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 6],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone7",
        relay_pin: 22,
        sensor_address: [0x28, 0, 0, 0, 0, 0, 0, 7],
        default_target_c: 21.1,
    },
];
*/

// =============================================================================
// WiFi Configuration (only for MQTT mode, not Matter)
// =============================================================================

/// WiFi SSID (set via WIFI_SSID environment variable at compile time)
/// Only required when not using Matter feature (Matter handles WiFi internally)
#[cfg(not(feature = "matter"))]
pub const WIFI_SSID: &str = env!("WIFI_SSID");

/// WiFi password (set via WIFI_PASSWORD environment variable at compile time)
/// Only required when not using Matter feature (Matter handles WiFi internally)
#[cfg(not(feature = "matter"))]
pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

// =============================================================================
// MQTT Configuration (available in both MQTT-only and Matter modes)
// =============================================================================

/// MQTT broker hostname or IP address (set via MQTT_BROKER_HOST env var, default: "localhost")
pub const MQTT_BROKER_HOST: &str = match option_env!("MQTT_BROKER_HOST") {
    Some(host) => host,
    None => "localhost",
};

/// MQTT broker port (default: 1883)
/// Note: Port cannot be easily set via env var due to const parsing limitations
pub const MQTT_BROKER_PORT: u16 = 1883;

/// MQTT client/device identifier (set via MQTT_DEVICE_ID env var, default: "emberheat-esp32")
pub const MQTT_DEVICE_ID: &str = match option_env!("MQTT_DEVICE_ID") {
    Some(id) => id,
    None => "emberheat-esp32",
};

/// MQTT username for authentication (set via MQTT_USERNAME env var, None = no auth)
pub const MQTT_USERNAME: Option<&str> = option_env!("MQTT_USERNAME");

/// MQTT password for authentication (set via MQTT_PASSWORD env var, None = no auth)
pub const MQTT_PASSWORD: Option<&str> = option_env!("MQTT_PASSWORD");

// =============================================================================
// Matter Configuration
// =============================================================================

/// Matter discriminator for commissioning (12-bit value, 0-4095)
///
/// Used in QR code and DNS-SD advertisement to identify the device during pairing.
/// Each device should have a unique discriminator for easy identification.
pub const MATTER_DISCRIMINATOR: u16 = 3840;

/// Matter setup passcode for commissioning (27-bit value, 1-99999998)
///
/// The passcode used during device pairing. This is the "PIN" shown on the device
/// or encoded in the QR code. Default is the Matter test passcode.
///
/// **Security note:** For production devices, use a unique passcode per device
/// and store it in secure flash.
pub const MATTER_PASSCODE: u32 = 20202021;

/// Matter vendor ID (0xFFF1-0xFFF4 are test vendor IDs)
///
/// Test vendor IDs are acceptable for development and personal use.
/// Production devices sold commercially need a registered vendor ID.
pub const MATTER_VENDOR_ID: u16 = 0xFFF1;

/// Matter product ID (test product ID)
pub const MATTER_PRODUCT_ID: u16 = 0x8001;

/// Device name shown during commissioning and in controller apps
pub const MATTER_DEVICE_NAME: &str = "EmberHeat Multi-Zone";

// =============================================================================
// Testing Configuration
// =============================================================================

/// Where zone temperatures come from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorSource {
    /// Fixed `MOCK_TEMPERATURE_C` (no hardware needed)
    Mock,
    /// Readings published to MQTT by remote sensors
    Mqtt,
}

/// Active sensor source. Stays `Mock` until `Mqtt` has been bench-tested.
pub const SENSOR_SOURCE: SensorSource = SensorSource::Mock;

/// Remote readings older than this (seconds) are ignored
pub const REMOTE_SENSOR_MAX_AGE_SECS: u64 = 600;

/// Maximum remote sensors averaged per zone
pub const MAX_REMOTE_SENSORS_PER_ZONE: usize = 4;

/// Duty-cycle fallback used when no fresh remote reading exists
pub const REMOTE_FALLBACK: emberheat_core::fallback::FallbackConfig =
    emberheat_core::fallback::FallbackConfig::DEFAULT;

// A hand-edited fallback config must not be able to heat continuously or
// divide by zero; catch it at build time rather than on the boiler.
const _: () = assert!(
    REMOTE_FALLBACK.max_duty >= 0.0 && REMOTE_FALLBACK.max_duty <= 1.0,
    "REMOTE_FALLBACK.max_duty must be within 0.0..=1.0"
);
const _: () = assert!(
    REMOTE_FALLBACK.period_secs > 0,
    "REMOTE_FALLBACK.period_secs must be greater than 0"
);

/// Republish each zone's sensor status this often (seconds) even when it has
/// not changed, so a restarted broker or Home Assistant picks it up again.
pub const SENSOR_STATUS_REPUBLISH_SECS: u64 = 300;

/// Temperature returned by mock sensors (in Celsius)
///
/// Only used when `SENSOR_SOURCE` is `SensorSource::Mock`.
pub const MOCK_TEMPERATURE_C: f32 = 20.0;

// =============================================================================
// Dual-Radio BLE Configuration (ESP32-H2 over UART HCI)
// =============================================================================

/// HCI UART baud rate (must match ESP32-H2 firmware)
/// Note: Using 115200 instead of 921600 to prevent FIFO overflow during commissioning.
/// The async UART read isn't polled fast enough at higher baud rates.
#[cfg(feature = "external-ble")]
pub const HCI_UART_BAUDRATE: u32 = 115200;

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

/// C6 pin wired to the H2's reset (EN/RST) pin.
///
/// The H2 keeps running while the C6 resets or is flashed, and reads the C6's
/// floating TX pin as garbage. H4 framing cannot recover from that, so the C6
/// restarts the H2 at every boot.
#[cfg(feature = "external-ble")]
pub const HCI_H2_RESET_PIN: u8 = 23;

/// How long the H2 reset line is held low (ms)
#[cfg(feature = "external-ble")]
pub const HCI_H2_RESET_PULSE_MS: u64 = 50;

/// Time allowed for the H2 to boot its HCI firmware after reset (ms)
#[cfg(feature = "external-ble")]
pub const HCI_H2_BOOT_MS: u64 = 1000;

/// RX FIFO fill level (bytes, of 128) at which the C6 raises RTS to pause the H2.
/// Above the 32-byte read threshold so normal reads never trip it.
#[cfg(feature = "external-ble")]
pub const HCI_UART_RTS_THRESHOLD: u8 = 96;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_num_zones() {
        // NUM_ZONES should match ZONE_CONFIGS length
        assert_eq!(ZONE_CONFIGS.len(), NUM_ZONES);
        // NUM_ZONES should be in valid range (1-16)
        assert!(
            NUM_ZONES >= 1 && NUM_ZONES <= 16,
            "NUM_ZONES must be between 1 and 16, got {}",
            NUM_ZONES
        );
    }

    #[test]
    fn test_zone_configs_have_unique_relay_pins() {
        let mut pins: [u8; NUM_ZONES] = [0; NUM_ZONES];
        for (i, config) in ZONE_CONFIGS.iter().enumerate() {
            pins[i] = config.relay_pin;
        }

        // Check for duplicates
        for i in 0..NUM_ZONES {
            for j in (i + 1)..NUM_ZONES {
                assert_ne!(
                    pins[i], pins[j],
                    "Duplicate relay pin {} in zones {} and {}",
                    pins[i], i, j
                );
            }
        }
    }

    #[test]
    fn test_zone_configs_have_unique_sensor_addresses() {
        for i in 0..NUM_ZONES {
            for j in (i + 1)..NUM_ZONES {
                assert_ne!(
                    ZONE_CONFIGS[i].sensor_address, ZONE_CONFIGS[j].sensor_address,
                    "Duplicate sensor address in zones {} and {}",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_all_sensors_are_ds18b20_family() {
        for (i, config) in ZONE_CONFIGS.iter().enumerate() {
            assert_eq!(
                config.sensor_address[0], 0x28,
                "Zone {} sensor should have DS18B20 family code 0x28",
                i
            );
        }
    }

    #[test]
    fn test_relay_pins_are_valid_gpio() {
        // ESP32-C6 valid GPIO pins (0-21)
        for config in ZONE_CONFIGS.iter() {
            assert!(
                config.relay_pin <= 21,
                "Relay pin {} is not a valid ESP32-C6 GPIO",
                config.relay_pin
            );
        }
    }

    #[test]
    fn test_onewire_pin_is_valid() {
        assert!(ONEWIRE_PIN <= 21, "1-Wire pin must be valid ESP32-C6 GPIO");
    }

    #[test]
    fn test_default_targets_are_reasonable() {
        for config in ZONE_CONFIGS.iter() {
            assert!(
                config.default_target_c >= 10.0 && config.default_target_c <= 30.0,
                "Default target {} is outside reasonable range",
                config.default_target_c
            );
        }
    }

    #[test]
    fn test_zone_config_data_is_copy() {
        let config = ZONE_CONFIGS[0];
        let _copy = config; // Should compile - ZoneConfigData is Copy
        let _ = config.name; // Original still usable
    }

    #[test]
    fn test_mqtt_config() {
        assert!(!MQTT_BROKER_HOST.is_empty());
        assert!(MQTT_BROKER_PORT > 0);
        assert!(!MQTT_DEVICE_ID.is_empty());
        // Username and password are optional, just verify they're valid Option types
        let _ = MQTT_USERNAME;
        let _ = MQTT_PASSWORD;
    }

    #[test]
    fn test_remote_sensor_limits() {
        assert!(MAX_REMOTE_SENSORS_PER_ZONE >= 1);
        assert!(REMOTE_SENSOR_MAX_AGE_SECS > 0);
        assert_eq!(SENSOR_SOURCE, SensorSource::Mock);
    }

    #[test]
    fn test_mock_temperature_is_reasonable() {
        assert!(
            MOCK_TEMPERATURE_C >= 10.0 && MOCK_TEMPERATURE_C <= 30.0,
            "Mock temperature {} is outside reasonable range",
            MOCK_TEMPERATURE_C
        );
    }
}
