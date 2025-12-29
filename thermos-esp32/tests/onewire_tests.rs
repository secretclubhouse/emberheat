//! Tests for DS18B20 1-Wire temperature sensor
//!
//! These tests validate the DS18B20 sensor implementation, focusing on:
//! - Temperature parsing from scratchpad data
//! - CRC verification
//! - Data format validation
//! - Configuration constants
//!
//! NOTE: These tests run in std environment and test the logic/algorithms,
//! not the actual hardware RMT peripheral.

// Test module configuration
#[cfg(test)]
mod ds18b20_tests {
    /// DS18B20 family code
    const DS18B20_FAMILY_CODE: u8 = 0x28;

    /// DS18B20 commands
    const CMD_CONVERT_T: u8 = 0x44;
    const CMD_READ_SCRATCHPAD: u8 = 0xBE;
    const CMD_SKIP_ROM: u8 = 0xCC;
    const CMD_MATCH_ROM: u8 = 0x55;

    /// Temperature conversion time for 12-bit resolution (max)
    const CONVERSION_TIME_MS: u64 = 750;

    /// Parse temperature from scratchpad data
    ///
    /// DS18B20 stores temperature as a 16-bit signed value in 1/16°C increments.
    fn parse_temperature(scratchpad: &[u8; 9]) -> f32 {
        let temp_lsb = scratchpad[0];
        let temp_msb = scratchpad[1];
        let raw = i16::from_le_bytes([temp_lsb, temp_msb]);
        raw as f32 / 16.0
    }

    /// Verify CRC of scratchpad data using Dallas/Maxim 1-Wire CRC-8
    fn verify_crc(data: &[u8; 9]) -> bool {
        let mut crc = 0u8;
        for i in 0..8 {
            let mut byte = data[i];
            for _ in 0..8 {
                let mix = (crc ^ byte) & 0x01;
                crc >>= 1;
                if mix != 0 {
                    crc ^= 0x8C;
                }
                byte >>= 1;
            }
        }
        crc == data[8]
    }

    // ========== Temperature Parsing Tests ==========

    #[test]
    fn test_parse_temperature_25c() {
        // 25.0625°C = 0x0191 = 401 decimal
        // 401 / 16 = 25.0625°C
        let scratchpad = [0x91, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - 25.0625).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_21_875c() {
        // 21.875°C = 0x015E = 350 decimal
        // Common room temperature test case
        let scratchpad = [0x5E, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert_eq!(temp, 21.875);
    }

    #[test]
    fn test_parse_temperature_negative_10_125c() {
        // -10.125°C = 0xFF5E = -162 decimal
        let scratchpad = [0x5E, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - (-10.125)).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_negative_55c() {
        // -55°C (minimum spec) = 0xFC90
        let scratchpad = [0x90, 0xFC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - (-55.0)).abs() < 0.1);
    }

    #[test]
    fn test_parse_temperature_125c() {
        // 125°C (maximum spec) = 0x07D0
        let scratchpad = [0xD0, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - 125.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_temperature_zero() {
        // 0°C = 0x0000
        let scratchpad = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert_eq!(temp, 0.0);
    }

    #[test]
    fn test_parse_temperature_0_5c() {
        // 0.5°C = 0x0008 = 8 decimal
        // Tests fractional parsing
        let scratchpad = [0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert_eq!(temp, 0.5);
    }

    #[test]
    fn test_parse_temperature_negative_0_5c() {
        // -0.5°C = 0xFFF8
        let scratchpad = [0xF8, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&scratchpad);
        assert_eq!(temp, -0.5);
    }

    #[test]
    fn test_temperature_resolution() {
        // DS18B20 resolution is 0.0625°C (1/16)
        let resolution = 1.0 / 16.0;
        assert_eq!(resolution, 0.0625);

        // Test that we can represent all resolution steps
        for i in -880..2000 {
            // -55°C to 125°C range in 1/16 increments
            let raw = i as i16;
            let bytes = raw.to_le_bytes();
            let scratchpad = [bytes[0], bytes[1], 0, 0, 0, 0, 0, 0, 0];
            let temp = parse_temperature(&scratchpad);
            let expected = i as f32 / 16.0;
            assert!((temp - expected).abs() < 0.0001);
        }
    }

    // ========== CRC Verification Tests ==========

    #[test]
    fn test_verify_crc_valid_25c() {
        // Real DS18B20 scratchpad: 25.0625°C
        // This is actual data from a DS18B20 sensor
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert!(verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_valid_21_875c() {
        // Real DS18B20 scratchpad: 21.875°C
        let scratchpad = [0x5E, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0xD9];
        assert!(verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_invalid() {
        // Same data but with wrong CRC
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0xFF];
        assert!(!verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_all_zeros() {
        // All zeros including CRC
        let scratchpad = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_algorithm() {
        // Test CRC algorithm with known values
        // CRC should be calculated over first 8 bytes
        let test_cases = vec![
            // [data bytes..., expected_crc]
            ([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], true),
            (
                [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00],
                false,
            ),
        ];

        for (data, expected) in test_cases {
            assert_eq!(verify_crc(&data), expected);
        }
    }

    // ========== Command and Constant Tests ==========

    #[test]
    fn test_family_code() {
        // DS18B20 family code is always 0x28
        assert_eq!(DS18B20_FAMILY_CODE, 0x28);
    }

    #[test]
    fn test_commands() {
        // Verify 1-Wire command codes
        assert_eq!(CMD_CONVERT_T, 0x44);
        assert_eq!(CMD_READ_SCRATCHPAD, 0xBE);
        assert_eq!(CMD_SKIP_ROM, 0xCC);
        assert_eq!(CMD_MATCH_ROM, 0x55);
    }

    #[test]
    fn test_conversion_time() {
        // 12-bit resolution requires 750ms max
        assert_eq!(CONVERSION_TIME_MS, 750);

        // Different resolution conversion times:
        // 9-bit: 93.75ms
        // 10-bit: 187.5ms
        // 11-bit: 375ms
        // 12-bit: 750ms (default)
    }

    // ========== Scratchpad Format Tests ==========

    #[test]
    fn test_scratchpad_size() {
        // DS18B20 scratchpad is always 9 bytes
        let scratchpad = [0u8; 9];
        assert_eq!(scratchpad.len(), 9);
    }

    #[test]
    fn test_scratchpad_structure() {
        // Scratchpad structure:
        // Byte 0: Temperature LSB
        // Byte 1: Temperature MSB
        // Byte 2: TH (high alarm)
        // Byte 3: TL (low alarm)
        // Byte 4: Configuration
        // Byte 5-7: Reserved (0xFF)
        // Byte 8: CRC

        let scratchpad = [
            0x91, // Temp LSB
            0x01, // Temp MSB (25.0625°C)
            0x4B, // TH
            0x46, // TL
            0x7F, // Config (12-bit resolution)
            0xFF, // Reserved
            0x0C, // Reserved
            0x10, // Reserved
            0x1C, // CRC
        ];

        // Verify temperature bytes
        assert_eq!(scratchpad[0], 0x91);
        assert_eq!(scratchpad[1], 0x01);

        // Verify CRC is last byte
        assert_eq!(scratchpad[8], 0x1C);

        // Parse and verify temperature
        let temp = parse_temperature(&scratchpad);
        assert!((temp - 25.0625).abs() < 0.001);

        // Verify CRC
        assert!(verify_crc(&scratchpad));
    }

    // ========== ROM Address Tests ==========

    #[test]
    fn test_rom_address_format() {
        // ROM address is 8 bytes:
        // [family_code, serial[6], crc]
        let rom = [
            0x28, // Family code (DS18B20)
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, // Serial number
            0xDE, // CRC
        ];

        assert_eq!(rom.len(), 8);
        assert_eq!(rom[0], DS18B20_FAMILY_CODE);
    }

    #[test]
    fn test_multiple_sensor_addresses() {
        // Simulate multiple sensors with different addresses
        let sensor1 = [0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
        let sensor2 = [0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x12];
        let sensor3 = [0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];

        // All should have DS18B20 family code
        assert_eq!(sensor1[0], DS18B20_FAMILY_CODE);
        assert_eq!(sensor2[0], DS18B20_FAMILY_CODE);
        assert_eq!(sensor3[0], DS18B20_FAMILY_CODE);

        // Serial numbers should be different
        assert_ne!(&sensor1[1..7], &sensor2[1..7]);
        assert_ne!(&sensor2[1..7], &sensor3[1..7]);
        assert_ne!(&sensor1[1..7], &sensor3[1..7]);
    }

    // ========== Temperature Range Tests ==========

    #[test]
    fn test_valid_temperature_range() {
        // DS18B20 operating range: -55°C to +125°C
        // Thermostat valid range: 3°C to 37°C (from thermos-core)

        let min_sensor = -55.0;
        let max_sensor = 125.0;
        let min_thermostat = 3.0;
        let max_thermostat = 37.0;

        assert!(min_sensor < min_thermostat);
        assert!(max_sensor > max_thermostat);

        // Common room temperatures should be in range
        assert!(20.0 >= min_thermostat && 20.0 <= max_thermostat);
        assert!(21.1 >= min_thermostat && 21.1 <= max_thermostat);
        assert!(25.0 >= min_thermostat && 25.0 <= max_thermostat);
    }

    #[test]
    fn test_temperature_out_of_thermostat_range() {
        // Temperatures that DS18B20 can measure but thermostat rejects
        let too_cold = 1.0; // Below 3°C
        let too_hot = 40.0; // Above 37°C

        assert!(too_cold < 3.0);
        assert!(too_hot > 37.0);
    }

    // ========== Configuration Tests ==========

    #[test]
    fn test_configuration_register() {
        // Configuration register (byte 4 of scratchpad)
        // Bits 6-5: Resolution
        //   00 = 9-bit (0.5°C)
        //   01 = 10-bit (0.25°C)
        //   10 = 11-bit (0.125°C)
        //   11 = 12-bit (0.0625°C) - default

        let config_9bit = 0b00011111; // R1=0, R0=0
        let config_10bit = 0b00111111; // R1=0, R0=1
        let config_11bit = 0b01011111; // R1=1, R0=0
        let config_12bit = 0b01111111; // R1=1, R0=1 (default)

        // Extract resolution bits
        assert_eq!((config_9bit >> 5) & 0b11, 0b00);
        assert_eq!((config_10bit >> 5) & 0b11, 0b01);
        assert_eq!((config_11bit >> 5) & 0b11, 0b10);
        assert_eq!((config_12bit >> 5) & 0b11, 0b11);
    }

    // ========== Error Handling Tests ==========

    #[test]
    fn test_corrupted_temperature_data() {
        // All 0xFF (sensor error or power issue)
        let scratchpad = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let temp = parse_temperature(&scratchpad);

        // 0xFFFF as signed 16-bit = -1
        // -1 / 16 = -0.0625
        assert_eq!(temp, -0.0625);

        // CRC will likely fail
        // In real implementation, CRC check happens first
    }

    #[test]
    fn test_temperature_parsing_edge_cases() {
        // Test boundary values for 16-bit signed integer

        // i16::MAX = 32767 / 16 = 2047.9375°C (beyond sensor spec)
        let max_i16 = [0xFF, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&max_i16);
        assert_eq!(temp, 2047.9375);

        // i16::MIN = -32768 / 16 = -2048°C (beyond sensor spec)
        let min_i16 = [0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = parse_temperature(&min_i16);
        assert_eq!(temp, -2048.0);
    }

    // ========== Multi-Sensor Tests ==========

    #[test]
    fn test_multiple_sensors_same_bus() {
        // Simulate reading from multiple sensors
        let sensor_readings = vec![
            ([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77], 21.5), // Living room
            ([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x12], 19.0), // Bedroom
            ([0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07], 23.0), // Kitchen
        ];

        for (addr, _expected_temp) in sensor_readings {
            assert_eq!(addr[0], DS18B20_FAMILY_CODE);
            assert_eq!(addr.len(), 8);
        }
    }

    // ========== Performance Tests ==========

    #[test]
    fn test_crc_calculation_performance() {
        // Verify CRC calculation is deterministic
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];

        // Multiple calls should give same result
        assert!(verify_crc(&scratchpad));
        assert!(verify_crc(&scratchpad));
        assert!(verify_crc(&scratchpad));
    }

    #[test]
    fn test_temperature_parsing_performance() {
        // Verify parsing is consistent
        let scratchpad = [0x91, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

        let temp1 = parse_temperature(&scratchpad);
        let temp2 = parse_temperature(&scratchpad);
        let temp3 = parse_temperature(&scratchpad);

        assert_eq!(temp1, temp2);
        assert_eq!(temp2, temp3);
    }
}
