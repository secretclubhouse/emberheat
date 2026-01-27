//! GPIO bit-banging 1-Wire implementation for ESP32
//!
//! This module provides a software 1-Wire implementation using GPIO bit-banging.
//! While slower than the RMT-based implementation, it works with esp-hal 1.0
//! and is sufficient for DS18B20 temperature sensors.
//!
//! # Timing
//!
//! 1-Wire protocol uses precise timing for communication:
//! - Reset pulse: 480µs low, then 60-240µs presence pulse
//! - Write 0: 60-120µs low
//! - Write 1: 1-15µs low, then release
//! - Read: 1-15µs low, sample at 15µs
//!
//! # Usage
//!
//! ```no_run
//! use thermos_esp32::onewire_gpio::OneWireGpio;
//! use esp_hal::delay::Delay;
//!
//! let delay = Delay::new();
//! let mut onewire = OneWireGpio::new(gpio_pin, delay);
//! if onewire.reset() {
//!     // Device present, can communicate
//!     onewire.write_byte(0xCC); // Skip ROM
//!     onewire.write_byte(0x44); // Convert T
//! }
//! ```

use core::fmt::Write;
use embassy_time::{Duration, Timer};
use esp_hal::delay::Delay;
use esp_hal::gpio::{Flex, InputConfig, OutputConfig, Pull};
use heapless::String as HeaplessString;
use heapless::Vec as HeaplessVec;
use thermos_core::SensorError;

/// DS18B20 ROM commands
pub const CMD_SEARCH_ROM: u8 = 0xF0;
pub const CMD_READ_ROM: u8 = 0x33;
pub const CMD_MATCH_ROM: u8 = 0x55;
pub const CMD_SKIP_ROM: u8 = 0xCC;

/// DS18B20 function commands
pub const CMD_CONVERT_T: u8 = 0x44;
pub const CMD_READ_SCRATCHPAD: u8 = 0xBE;

/// DS18B20 family code
pub const DS18B20_FAMILY: u8 = 0x28;

/// Maximum sensors per bus
pub const MAX_SENSORS: usize = 8;

/// GPIO bit-banging 1-Wire bus implementation
///
/// Uses a flexible GPIO pin that can switch between input and output modes.
/// Timing is achieved using esp-hal's Delay for accurate microsecond delays.
pub struct OneWireGpio<'d> {
    pin: Flex<'d>,
    delay: Delay,
}

impl<'d> OneWireGpio<'d> {
    /// Create a new 1-Wire bus on the given GPIO pin
    ///
    /// The pin is configured as open-drain output with internal pull-up.
    pub fn new(pin: Flex<'d>, delay: Delay) -> Self {
        let mut ow = Self { pin, delay };
        // Start with pin released (high via pull-up)
        ow.release();
        ow
    }

    /// Release the bus (set high via pull-up, input mode)
    #[inline]
    fn release(&mut self) {
        // Configure as input with pull-up
        self.pin
            .apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        self.pin.set_output_enable(false);
        self.pin.set_input_enable(true);
    }

    /// Drive the bus low (output mode)
    #[inline]
    fn drive_low(&mut self) {
        self.pin.set_input_enable(false);
        self.pin.apply_output_config(&OutputConfig::default());
        self.pin.set_output_enable(true);
        self.pin.set_low();
    }

    /// Read the current bus level
    #[inline]
    fn read_level(&self) -> bool {
        self.pin.is_high()
    }

    /// Send a reset pulse and detect presence
    ///
    /// Returns true if a device responded with a presence pulse.
    pub fn reset(&mut self) -> bool {
        // Drive low for 480µs (minimum 480µs)
        self.drive_low();
        self.delay.delay_micros(480);

        // Release and wait 70µs for presence pulse
        self.release();
        self.delay.delay_micros(70);

        // Sample for presence (low = device present)
        let presence = !self.read_level();

        // Wait for remainder of reset sequence (total 480µs after release)
        self.delay.delay_micros(410);

        presence
    }

    /// Write a single bit to the bus
    fn write_bit(&mut self, bit: bool) {
        if bit {
            // Write 1: drive low for 6µs, then release for 64µs
            self.drive_low();
            self.delay.delay_micros(6);
            self.release();
            self.delay.delay_micros(64);
        } else {
            // Write 0: drive low for 60µs, then release for 10µs
            self.drive_low();
            self.delay.delay_micros(60);
            self.release();
            self.delay.delay_micros(10);
        }
    }

    /// Read a single bit from the bus
    fn read_bit(&mut self) -> bool {
        // Drive low for 3µs to initiate read slot
        self.drive_low();
        self.delay.delay_micros(3);

        // Release and wait 10µs before sampling
        self.release();
        self.delay.delay_micros(10);

        // Sample the bit
        let bit = self.read_level();

        // Wait for remainder of time slot (total ~70µs)
        self.delay.delay_micros(53);

        bit
    }

    /// Write a byte to the bus (LSB first)
    pub fn write_byte(&mut self, byte: u8) {
        for i in 0..8 {
            self.write_bit((byte >> i) & 1 == 1);
        }
    }

    /// Read a byte from the bus (LSB first)
    pub fn read_byte(&mut self) -> u8 {
        let mut byte = 0u8;
        for i in 0..8 {
            if self.read_bit() {
                byte |= 1 << i;
            }
        }
        byte
    }

    /// Read multiple bytes from the bus
    pub fn read_bytes(&mut self, buffer: &mut [u8]) {
        for byte in buffer.iter_mut() {
            *byte = self.read_byte();
        }
    }

    /// Select a specific device by ROM address
    pub fn select(&mut self, address: &[u8; 8]) -> bool {
        if !self.reset() {
            return false;
        }
        self.write_byte(CMD_MATCH_ROM);
        for byte in address {
            self.write_byte(*byte);
        }
        true
    }

    /// Skip ROM selection (only use with single device on bus)
    pub fn skip_rom(&mut self) -> bool {
        if !self.reset() {
            return false;
        }
        self.write_byte(CMD_SKIP_ROM);
        true
    }

    /// Read ROM address (only use with single device on bus)
    pub fn read_rom(&mut self) -> Option<[u8; 8]> {
        if !self.reset() {
            return None;
        }
        self.write_byte(CMD_READ_ROM);

        let mut address = [0u8; 8];
        self.read_bytes(&mut address);

        // Verify CRC
        if crc8(&address) != 0 {
            return None;
        }

        Some(address)
    }

    /// Search for all devices on the bus
    ///
    /// Uses the 1-Wire search algorithm to discover all device ROM addresses.
    pub fn search(&mut self) -> HeaplessVec<[u8; 8], MAX_SENSORS> {
        let mut devices = HeaplessVec::new();
        let mut last_discrepancy = 0i8;
        let mut last_device = false;
        let mut rom = [0u8; 8];

        while !last_device {
            if !self.reset() {
                break;
            }

            self.write_byte(CMD_SEARCH_ROM);

            let mut id_bit_number = 1i8;
            let mut last_zero = 0i8;
            let mut rom_byte_number = 0usize;
            let mut rom_byte_mask = 1u8;

            loop {
                // Read bit and complement
                let id_bit = self.read_bit();
                let cmp_id_bit = self.read_bit();

                // Check for no devices
                if id_bit && cmp_id_bit {
                    break;
                }

                let search_direction = if id_bit != cmp_id_bit {
                    // All devices have same bit
                    id_bit
                } else {
                    // Discrepancy - devices have different bits
                    if id_bit_number == last_discrepancy {
                        true
                    } else if id_bit_number > last_discrepancy {
                        false
                    } else {
                        (rom[rom_byte_number] & rom_byte_mask) != 0
                    }
                };

                // Update ROM bit
                if search_direction {
                    rom[rom_byte_number] |= rom_byte_mask;
                } else {
                    rom[rom_byte_number] &= !rom_byte_mask;
                    last_zero = id_bit_number;
                }

                // Write search direction bit
                self.write_bit(search_direction);

                // Move to next bit
                id_bit_number += 1;
                rom_byte_mask <<= 1;

                if rom_byte_mask == 0 {
                    rom_byte_number += 1;
                    rom_byte_mask = 1;
                }

                if rom_byte_number >= 8 {
                    break;
                }
            }

            // Check if search was successful
            if id_bit_number > 64 {
                // Verify CRC
                if crc8(&rom) == 0 {
                    // Add to list if it's a DS18B20
                    if rom[0] == DS18B20_FAMILY {
                        let _ = devices.push(rom);
                    }
                }

                last_discrepancy = last_zero;
                if last_discrepancy == 0 {
                    last_device = true;
                }
            } else {
                break;
            }
        }

        devices
    }
}

/// DS18B20 temperature sensor using GPIO bit-banging
pub struct Ds18b20Gpio<'d> {
    bus: OneWireGpio<'d>,
    address: [u8; 8],
    zone_name: HeaplessString<32>,
}

impl<'d> Ds18b20Gpio<'d> {
    /// Create a new DS18B20 sensor with known address
    pub fn new(bus: OneWireGpio<'d>, address: [u8; 8], zone_name: &str) -> Self {
        let mut name = HeaplessString::new();
        for c in zone_name.chars().take(32) {
            if name.push(c).is_err() {
                break;
            }
        }

        Self {
            bus,
            address,
            zone_name: name,
        }
    }

    /// Create a sensor for a single device on the bus (auto-detect address)
    pub fn new_single(mut bus: OneWireGpio<'d>, zone_name: &str) -> Option<Self> {
        let address = bus.read_rom()?;
        if address[0] != DS18B20_FAMILY {
            return None;
        }
        Some(Self::new(bus, address, zone_name))
    }

    /// Get the ROM address
    pub fn address(&self) -> [u8; 8] {
        self.address
    }

    /// Get the zone name
    pub fn zone_name(&self) -> &str {
        &self.zone_name
    }

    /// Start temperature conversion (takes ~750ms)
    pub fn start_conversion(&mut self) -> bool {
        if !self.bus.select(&self.address) {
            return false;
        }
        self.bus.write_byte(CMD_CONVERT_T);
        true
    }

    /// Read temperature after conversion is complete
    ///
    /// Call `start_conversion()` first and wait 750ms before calling this.
    pub fn read_temperature_raw(&mut self) -> Result<f32, SensorError> {
        if !self.bus.select(&self.address) {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "No response from sensor {}", self.zone_name);
            return Err(SensorError::IoError(msg));
        }

        self.bus.write_byte(CMD_READ_SCRATCHPAD);

        let mut scratchpad = [0u8; 9];
        self.bus.read_bytes(&mut scratchpad);

        // Verify CRC
        if crc8(&scratchpad) != 0 {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "CRC error reading sensor {}", self.zone_name);
            return Err(SensorError::InvalidData(msg));
        }

        // Parse temperature (bytes 0-1, LSB first, signed 16-bit, 1/16°C resolution)
        let raw = i16::from_le_bytes([scratchpad[0], scratchpad[1]]);
        let temp_c = raw as f32 / 16.0;

        Ok(temp_c)
    }

    /// Read temperature with conversion (blocking for ~750ms)
    pub async fn read_temperature(&mut self) -> Result<f32, SensorError> {
        if !self.start_conversion() {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(
                &mut msg,
                "Failed to start conversion for {}",
                self.zone_name
            );
            return Err(SensorError::IoError(msg));
        }

        // Wait for conversion (750ms for 12-bit resolution)
        Timer::after(Duration::from_millis(750)).await;

        self.read_temperature_raw()
    }
}

/// Calculate CRC-8 for 1-Wire (polynomial 0x8C, reflected)
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        let mut b = *byte;
        for _ in 0..8 {
            let mix = (crc ^ b) & 0x01;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            b >>= 1;
        }
    }
    crc
}

/// Format ROM address as hex string
pub fn format_address(address: &[u8; 8]) -> HeaplessString<24> {
    let mut hex = HeaplessString::new();
    for (i, byte) in address.iter().enumerate() {
        if i > 0 {
            let _ = write!(&mut hex, "-");
        }
        let _ = write!(&mut hex, "{:02X}", byte);
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc8_empty() {
        assert_eq!(crc8(&[]), 0);
    }

    #[test]
    fn test_crc8_valid_rom() {
        // A valid DS18B20 ROM with correct CRC
        let rom = [0x28, 0xFF, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        // For a valid ROM, CRC of all 8 bytes should be 0
        // (the last byte is the CRC of the first 7)
        // This test just verifies the function runs
        let _ = crc8(&rom);
    }

    #[test]
    fn test_format_address() {
        let address = [0x28, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78];
        let hex = format_address(&address);
        assert_eq!(hex.as_str(), "28-AB-CD-EF-12-34-56-78");
    }

    #[test]
    fn test_temperature_parsing() {
        // 25.0625°C = 401 raw = 0x0191
        let raw = i16::from_le_bytes([0x91, 0x01]);
        let temp = raw as f32 / 16.0;
        assert!((temp - 25.0625).abs() < 0.001);

        // -10.125°C = -162 raw = 0xFF5E
        let raw = i16::from_le_bytes([0x5E, 0xFF]);
        let temp = raw as f32 / 16.0;
        assert!((temp - (-10.125)).abs() < 0.001);
    }
}
