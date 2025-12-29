# DS18B20 1-Wire Temperature Sensor Implementation for ESP32-C6

## Overview

This document describes the DS18B20 temperature sensor implementation for ESP32-C6 using the RMT (Remote Control) peripheral for precise 1-Wire protocol timing.

## Implementation Details

### Library: esp-hal-rmt-onewire v0.4.0

The implementation uses `esp-hal-rmt-onewire`, a Rust library that provides:
- Hardware-based 1-Wire timing via ESP32 RMT peripheral
- Async/await support with Embassy runtime
- DS18B20-specific operations (ROM search, temperature conversion)
- Reliable timing without CPU intervention

**Why RMT peripheral?**
- **Precise timing**: 1-Wire protocol requires microsecond-level timing accuracy
- **Hardware offload**: RMT handles protocol timing in hardware, freeing CPU
- **Reliability**: Not affected by CPU load or interrupts
- **No bit-banging**: Software timing is error-prone on ESP32

### Sensor Capabilities

**DS18B20 Specifications:**
- Temperature range: -55°C to +125°C
- Resolution: 0.0625°C (12-bit, default)
- Accuracy: ±0.5°C from -10°C to +85°C
- Conversion time: 750ms max (12-bit resolution)
- Interface: 1-Wire bus (single data line + GND)
- Power: 3.0V to 5.5V (can use parasite power)

**Thermostat Valid Range:**
- Operating range: 3°C to 37°C (from thermos-core)
- Sensor readings outside this range are rejected

### Hardware Connection

```
ESP32-C6          DS18B20
========          =======
GPIO6 ----------- Data (yellow/white)
GND ------------- GND (black)
3.3V ------------ VDD (red) [optional if using parasite power]

Required: 4.7K Ohm pull-up resistor between Data and VDD
```

**GPIO Pin Selection:**
- Any GPIO pin can be used for 1-Wire
- GPIO6 is used in examples (can be changed)
- Pull-up resistor is required (4.7K typical)

**RMT Channels:**
- Uses 2 RMT channels (TX and RX)
- ESP32-C6 has 4 RMT channels total (0-3)
- Channels 0-1: TX (transmit)
- Channels 2-3: RX (receive)

## Code Structure

### Module: `thermos-esp32/src/sensor_onewire.rs`

**Main Components:**

#### 1. Ds18b20Sensor struct
```rust
pub struct Ds18b20Sensor<'a> {
    onewire: OneWire<'a>,           // RMT-based 1-Wire bus
    address: Option<Address>,        // Sensor ROM address (8 bytes)
    sensor_id: HeaplessString<32>,  // Human-readable ID
}
```

#### 2. Initialization Methods

**Single sensor (auto-discover):**
```rust
let rmt = Rmt::new(peripherals.RMT, 80.MHz());
let mut sensor = Ds18b20Sensor::new(rmt, gpio_pin, "living-room")?;
sensor.init().await?;  // Searches for first DS18B20 on bus
```

**Multiple sensors (specific address):**
```rust
let mut sensor = Ds18b20Sensor::new(rmt, gpio_pin, "bedroom")?;
let address = [0x28, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
sensor.init_with_address(address)?;  // Use specific sensor
```

#### 3. Temperature Reading

Implements `TemperatureSensor` trait from `thermos-core`:

```rust
async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
    // 1. Send CONVERT_T command (0x44)
    // 2. Wait 750ms for conversion
    // 3. Read 9-byte scratchpad
    // 4. Verify CRC
    // 5. Parse temperature (16-bit signed, 1/16°C increments)
    // 6. Validate range (3°C to 37°C)
}
```

#### 4. CRC Verification

Uses Dallas/Maxim 1-Wire CRC-8 algorithm:
```rust
fn verify_crc(data: &[u8; 9]) -> bool {
    // CRC-8 with polynomial 0x8C
    // Calculated over first 8 bytes
    // Compared with byte 9
}
```

### Scratchpad Format

DS18B20 scratchpad is 9 bytes:

```
Byte 0: Temperature LSB
Byte 1: Temperature MSB
Byte 2: TH (high alarm register)
Byte 3: TL (low alarm register)
Byte 4: Configuration (resolution bits 6-5)
Byte 5-7: Reserved (0xFF)
Byte 8: CRC
```

**Temperature Calculation:**
```rust
let raw = i16::from_le_bytes([lsb, msb]);
let celsius = raw as f32 / 16.0;
```

### ROM Address Format

Each DS18B20 has a unique 64-bit ROM address:

```
Byte 0: Family code (0x28 for DS18B20)
Bytes 1-6: Serial number (unique to each sensor)
Byte 7: CRC
```

**Example:** `[0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]`

### Resolution Configuration

DS18B20 supports 4 resolution levels (configuration register byte 4, bits 6-5):

| Bits R1:R0 | Resolution | Conversion Time | Increment |
|-----------|------------|----------------|-----------|
| 00        | 9-bit      | 93.75ms        | 0.5°C     |
| 01        | 10-bit     | 187.5ms        | 0.25°C    |
| 10        | 11-bit     | 375ms          | 0.125°C   |
| 11        | 12-bit     | 750ms          | 0.0625°C  |

**Default:** 12-bit (highest resolution, 750ms conversion)

## Usage Examples

### Example 1: Single Sensor

```rust
use thermos_esp32::Ds18b20Sensor;
use thermos_core::TemperatureSensor;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init(Config::default());
    
    // Initialize RMT at 80 MHz
    let rmt = Rmt::new(peripherals.RMT, 80.MHz());
    
    // Create sensor
    let mut sensor = Ds18b20Sensor::new(
        rmt, 
        peripherals.GPIO6,
        "zone1"
    ).unwrap();
    
    // Discover sensor on bus
    sensor.init().await.unwrap();
    
    // Read temperature
    let reading = sensor.read_temperature().await.unwrap();
    esp_println::println!("Temperature: {:.1}°C / {:.1}°F", 
        reading.celsius, reading.fahrenheit);
}
```

### Example 2: Multiple Sensors

```rust
// Scan bus for all DS18B20 sensors
let addresses = sensor.list_sensors().await?;
esp_println::println!("Found {} sensors", addresses.len());

// Create sensor for each address
for addr in addresses {
    let mut s = Ds18b20Sensor::new(rmt, gpio_pin, "sensor")?;
    s.init_with_address(addr)?;
    
    let reading = s.read_temperature().await?;
    esp_println::println!("Sensor {:02X?}: {:.1}°C", addr, reading.celsius);
}
```

### Example 3: Control Loop Integration

```rust
#[embassy_executor::task]
async fn zone_control(sensor: Ds18b20Sensor<'static>, relay: Esp32GpioPin) {
    let mut thermostat = Thermostat::new("Zone1", 21.1);
    
    loop {
        // Read temperature
        match sensor.read_temperature().await {
            Ok(reading) => {
                // Update thermostat state
                thermostat.update_temperature(reading.celsius);
                
                // Control heating
                let heating = thermostat.should_heat();
                relay.set(heating).await.unwrap();
                
                esp_println::println!("Zone1: {:.1}°C, heating: {}", 
                    reading.celsius, heating);
            }
            Err(e) => {
                esp_println::println!("Sensor error: {:?}", e);
            }
        }
        
        // 6-second control loop
        Timer::after(Duration::from_secs(6)).await;
    }
}
```

## Testing

### Unit Tests (thermos-esp32/tests/onewire_tests.rs)

The test suite includes 45+ tests covering:

**Temperature Parsing:**
- Positive temperatures (0°C, 21.875°C, 25.0625°C, 125°C)
- Negative temperatures (-0.5°C, -10.125°C, -55°C)
- Edge cases (zero, min, max, fractional values)
- Resolution verification (0.0625°C increments)

**CRC Verification:**
- Valid scratchpad data with correct CRC
- Invalid scratchpad data (wrong CRC)
- All zeros case
- Algorithm correctness

**Data Format:**
- Scratchpad structure (9 bytes)
- ROM address format (8 bytes, family code 0x28)
- Configuration register bits
- Command codes (0x44, 0xBE, 0xCC, 0x55)

**Multi-Sensor:**
- Multiple sensors on same bus
- Unique serial numbers
- Address filtering by family code

**Running Tests:**
```bash
cd thermos-esp32
cargo test --test onewire_tests
```

**Note:** Tests are pure logic tests (std environment) that validate:
- Temperature calculation algorithm
- CRC verification algorithm
- Data parsing
- Constants and configurations

Tests do **not** require ESP32 hardware or RMT peripheral.

## Error Handling

### SensorError Types

```rust
pub enum SensorError {
    IoError(HeaplessString<128>),      // 1-Wire communication failed
    InvalidData(HeaplessString<128>),  // CRC check failed
    OutOfRange(f32),                   // Temperature outside 3-37°C
    Timeout,                           // (not used in DS18B20)
}
```

### Common Errors

**1. No device present:**
```
SensorError::IoError("1-Wire reset failed - no device present")
```
- **Cause:** Sensor not connected, wrong GPIO pin, or no pull-up resistor
- **Fix:** Check wiring, verify 4.7K pull-up resistor is present

**2. CRC check failed:**
```
SensorError::InvalidData("Scratchpad CRC check failed")
```
- **Cause:** Data corruption on 1-Wire bus
- **Fix:** Shorten wires, add filtering capacitor, check power supply

**3. No DS18B20 found:**
```
SensorError::IoError("No DS18B20 sensors found on bus")
```
- **Cause:** Wrong sensor type (not DS18B20) or bus scan failed
- **Fix:** Verify sensor is DS18B20 (family code 0x28)

**4. Temperature out of range:**
```
SensorError::OutOfRange(40.5)
```
- **Cause:** Temperature reading outside thermostat's valid range (3-37°C)
- **Fix:** This is expected behavior for extreme temperatures

### Recovery Strategies

**1. Retry on communication error:**
```rust
for attempt in 0..3 {
    match sensor.read_temperature().await {
        Ok(reading) => return Ok(reading),
        Err(SensorError::IoError(_)) => {
            Timer::after(Duration::from_millis(500)).await;
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

**2. Fallback sensor:**
```rust
let reading = match primary_sensor.read_temperature().await {
    Ok(r) => r,
    Err(_) => backup_sensor.read_temperature().await?,
};
```

## Performance Considerations

### Timing

- **Conversion time:** 750ms (12-bit resolution)
- **Read scratchpad:** ~5ms (9 bytes at 1-Wire speed)
- **Total read time:** ~755ms per sensor
- **Control loop:** 6 seconds (sensor read is 12.5% of loop)

### Memory Usage

- **Ds18b20Sensor struct:** ~100 bytes
- **OneWire state:** ~50 bytes (RMT buffers)
- **Scratchpad buffer:** 9 bytes (stack)
- **Total per sensor:** ~160 bytes

### Multi-Sensor Performance

With multiple sensors on same bus:
- **Sequential reads:** 755ms × N sensors
- **Parallel conversion:** Start all sensors simultaneously (CMD_SKIP_ROM), wait 750ms, then read each (~5ms each)
- **Example:** 5 sensors = 750ms + (5 × 5ms) = ~775ms total

**Optimization:**
```rust
// 1. Start conversion on all sensors
sensor.start_conversion().await?;  // Uses CMD_SKIP_ROM
Timer::after(Duration::from_millis(750)).await;

// 2. Read each sensor sequentially
for addr in addresses {
    sensor.init_with_address(addr)?;
    let scratchpad = sensor.read_scratchpad().await?;
    // ... process temperature
}
```

## Troubleshooting

### Issue: Sensor not detected

**Symptoms:** `init()` fails with "No DS18B20 sensors found"

**Checks:**
1. Verify wiring (data, GND, VDD)
2. Check 4.7K pull-up resistor is present
3. Verify GPIO pin number is correct
4. Test with multimeter: Data line should be ~3.3V when idle
5. Try different GPIO pin
6. Reduce wire length (<5m recommended)

### Issue: CRC errors

**Symptoms:** Frequent "CRC check failed" errors

**Checks:**
1. Shorten wire length
2. Add 100nF capacitor across VDD and GND at sensor
3. Use shielded cable for long runs
4. Check power supply is stable (3.0-5.5V)
5. Reduce wire gauge (thicker is better)

### Issue: Incorrect temperatures

**Symptoms:** Temperature readings are wrong

**Checks:**
1. Verify sensor is genuine DS18B20 (counterfeit sensors exist)
2. Check temperature calculation algorithm
3. Try different resolution (9-bit vs 12-bit)
4. Compare with known-good thermometer
5. Check for self-heating (sensor power consumption)

### Issue: Slow response

**Symptoms:** Temperature changes slowly

**Explanation:** This is normal behavior!
- DS18B20 has thermal mass (takes time to equilibrate)
- 750ms conversion time is per reading
- Sensor packaging affects response time
- Typical time constant: 5-10 seconds in air, 0.5-2 seconds in liquid

## References

### Datasheets and Documentation

- [DS18B20 Datasheet](https://www.analog.com/media/en/technical-documentation/data-sheets/DS18B20.pdf) - Maxim Integrated
- [1-Wire Protocol](https://www.analog.com/media/en/technical-documentation/app-notes/AN126.pdf) - Application Note 126
- [esp-hal RMT Documentation](https://docs.rs/esp-hal/latest/esp_hal/rmt/index.html)
- [esp-hal-rmt-onewire](https://docs.rs/esp-hal-rmt-onewire/latest/esp_hal_rmt_onewire/)
- [ESP32-C6 RMT Peripheral](https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-reference/peripherals/rmt.html)

### Related Code

- `thermos-esp32/src/sensor_onewire.rs` - DS18B20 implementation
- `thermos-esp32/tests/onewire_tests.rs` - Test suite
- `thermos-core/src/sensor.rs` - TemperatureSensor trait
- `thermos-linux/src/sensors/onewire.rs` - Linux /sys/bus/w1 implementation

### Example Projects

- [esp-hal-rmt-onewire examples](https://github.com/jonored/esp-hal-rmt-onewire/tree/main/examples) - thermometer.rs
- [esp32temp](https://github.com/sjm42/esp32temp) - Multi-sensor ESP32 thermometer
- [esp32c3-embassy](https://github.com/claudiomattera/esp32c3-embassy) - Embassy async examples

## Migration from Raspberry Pi

### Differences from Linux Implementation

**Raspberry Pi (thermos-linux/src/sensors/onewire.rs):**
- Uses kernel `w1` driver
- Reads from `/sys/bus/w1/devices/28-*/w1_slave`
- Kernel handles 1-Wire protocol
- Blocking I/O with tokio

**ESP32-C6 (thermos-esp32/src/sensor_onewire.rs):**
- Uses RMT hardware peripheral
- Direct 1-Wire protocol implementation
- Async I/O with Embassy
- Must handle CRC, timing, protocol details

### Code Comparison

**Raspberry Pi:**
```rust
// Kernel does all the work
let content = fs::read_to_string("/sys/bus/w1/devices/28-xxx/w1_slave").await?;
let temp = parse_temperature(&content)?;  // Parse kernel's output
```

**ESP32-C6:**
```rust
// We handle the protocol
sensor.start_conversion().await?;
Timer::after(Duration::from_millis(750)).await;
let scratchpad = sensor.read_scratchpad().await?;
verify_crc(&scratchpad)?;
let temp = parse_temperature(&scratchpad);
```

### Shared Code

Both implementations:
- Implement `TemperatureSensor` trait
- Use same temperature ranges (3-37°C)
- Return `TemperatureReading` struct
- Handle errors via `SensorError`
- Support sensor ID strings

## Future Enhancements

### Planned Features

1. **Configurable resolution** - Allow 9/10/11-bit for faster readings
2. **Alarm thresholds** - Use DS18B20's TH/TL registers
3. **Parallel conversion** - Start all sensors simultaneously
4. **EEPROM persistence** - Save configuration to sensor
5. **Power management** - Support for parasite power mode

### Not Planned

- Temperature logging (use MQTT instead)
- Trend analysis (done in MQTT consumer)
- Web interface (use HomeKit/Matter instead)
