//! RMT-based 1-Wire driver for ESP32
//!
//! This is a vendored and adapted version of esp-hal-rmt-onewire from:
//! https://github.com/willpuckett/esp-hal-rmt-onewire
//! (PR #2 updating for esp-hal 1.0)
//!
//! Uses the RMT peripheral for precise 1-Wire timing, which is more reliable
//! than GPIO bit-banging.

use embassy_futures::select::*;
use esp_hal::{
    gpio::{
        interconnect::*, DriveMode, DriveStrength, Flex, InputConfig, Level, OutputConfig, Pin,
        Pull,
    },
    rmt::{
        Channel, PulseCode, Rx, RxChannelConfig, RxChannelCreator, Tx, TxChannelConfig,
        TxChannelCreator,
    },
    Async,
};

pub struct OneWire<'a> {
    rx: Channel<'a, Async, Rx>,
    tx: Channel<'a, Async, Tx>,
    input: InputSignal<'a>,
}

const MAX_EXCHANGE_PULSES: usize = 65;

impl<'a> OneWire<'a> {
    pub fn new<Txc: TxChannelCreator<'a, Async>, Rxc: RxChannelCreator<'a, Async>, P: Pin + 'a>(
        txcc: Txc,
        rxcc: Rxc,
        pin: P,
    ) -> Result<Self, Error> {
        let rx_config = RxChannelConfig::default()
            .with_clk_divider(80)
            .with_idle_threshold(1000)
            .with_filter_threshold(10)
            .with_carrier_modulation(false);
        let tx_config = TxChannelConfig::default()
            .with_clk_divider(80)
            .with_carrier_modulation(false);

        let mut pin: Flex = Flex::new(pin);

        pin.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        pin.apply_output_config(
            &OutputConfig::default()
                .with_drive_mode(DriveMode::OpenDrain)
                .with_drive_strength(DriveStrength::_40mA),
        );
        pin.set_input_enable(true);
        pin.set_output_enable(true);
        let (input, output) = pin.split();

        let tx = txcc
            .configure_tx(&tx_config)
            .map_err(Error::ConfigError)?
            .with_pin(output.with_output_inverter(true));
        let rx = rxcc
            .configure_rx(&rx_config)
            .map_err(Error::ConfigError)?
            .with_pin(input.clone().with_input_inverter(true));

        Ok(OneWire { rx, tx, input })
    }
}

impl<'a> OneWire<'a> {
    pub async fn reset(&mut self) -> Result<bool, Error> {
        let data = [
            PulseCode::new(Level::Low, 60, Level::High, 600),
            PulseCode::new(Level::Low, 600, Level::Low, 0),
            PulseCode::end_marker(),
        ];
        let mut indata = [PulseCode::end_marker(); 10];

        let _res = self.send_and_receive(&mut indata, &data).await?;

        let sample0 = indata[0];
        let sample1 = indata[1];
        const PRESENCE_MIN_TICKS: u16 = 30; // 30 µs with 1 MHz tick
        const PRESENCE_MAX_TICKS: u16 = 500; // generous upper bound for noisy buses

        Ok(sample0.length1() > 0
            && sample0.length2() > 0
            && sample1.length1() >= PRESENCE_MIN_TICKS
            && sample1.length1() <= PRESENCE_MAX_TICKS)
    }

    pub async fn send_and_receive(
        &mut self,
        indata: &mut [PulseCode],
        data: &[PulseCode],
    ) -> Result<(), Error> {
        let delay = [PulseCode::new(Level::Low, 30000, Level::Low, 0)]; // timeout delay for 30ms using the RMT tx peripheral.
        if self.input.level() == Level::Low {
            Err(Error::InputNotHigh)?;
        }
        // This relies on select polling in order to set up the rx & tx registers, which is not strictly documented behavior.
        let res = select(self.rx.receive(indata), async {
            let r = self.tx.transmit(data).await;
            let _ = self.tx.transmit(&delay).await;
            r
        })
        .await;

        match res {
            Either::First(Ok(_)) => Ok(()),
            Either::First(Err(r)) => Err(Error::ReceiveError(r)),
            Either::Second(Ok(())) => Err(Error::ReceiveTimedOut),
            Either::Second(Err(e)) => Err(Error::SendError(e)),
        }
    }

    const ZERO_BIT_LEN: u16 = 70;
    const ONE_BIT_LEN: u16 = 3;

    pub fn encode_bit(bit: bool) -> PulseCode {
        if bit {
            PulseCode::new(
                Level::High,
                Self::ONE_BIT_LEN,
                Level::Low,
                Self::ZERO_BIT_LEN,
            )
        } else {
            PulseCode::new(
                Level::High,
                Self::ZERO_BIT_LEN,
                Level::Low,
                Self::ONE_BIT_LEN,
            )
        }
    }

    pub fn decode_bit(code: PulseCode) -> bool {
        let len = code.length1();
        if len < 20 {
            true
        } else {
            false
        }
    }

    pub async fn exchange_byte(&mut self, byte: u8) -> Result<u8, Error> {
        let mut data = [PulseCode::end_marker(); 10];
        let mut indata = [PulseCode::end_marker(); 10];
        for n in 0..8 {
            data[n] = Self::encode_bit(0 != byte & 1 << n);
        }
        let _res = self.send_and_receive(&mut indata, &data).await?;
        let mut res: u8 = 0;
        for n in 0..8 {
            if Self::decode_bit(indata[n]) {
                res |= 1 << n;
            }
        }
        Ok(res)
    }

    pub async fn send_byte(&mut self, byte: u8) -> Result<(), Error> {
        let mut data = [PulseCode::end_marker(); 10];
        for n in 0..8 {
            data[n] = Self::encode_bit(0 != byte & 1 << n);
        }
        let _res = self.tx.transmit(&data).await?;
        Ok(())
    }

    pub async fn exchange_bits<const N: usize>(
        &mut self,
        bits: [bool; N],
    ) -> Result<[bool; N], Error> {
        assert!(N + 1 <= MAX_EXCHANGE_PULSES);

        let mut tx_buf = [PulseCode::end_marker(); MAX_EXCHANGE_PULSES];
        let mut rx_buf = [PulseCode::end_marker().into(); MAX_EXCHANGE_PULSES];

        for (idx, bit) in bits.into_iter().enumerate() {
            tx_buf[idx] = Self::encode_bit(bit);
        }
        tx_buf[N] = PulseCode::end_marker();

        self.send_and_receive(&mut rx_buf[..=N], &tx_buf[..=N])
            .await?;

        let mut res: [bool; N] = [false; N];
        for n in 0..N {
            res[n] = Self::decode_bit(PulseCode::from(rx_buf[n]));
        }
        Ok(res)
    }

    pub async fn send_u64(&mut self, val: u64) -> Result<(), Error> {
        for byte in val.to_le_bytes() {
            self.send_byte(byte).await?;
        }
        Ok(())
    }

    pub async fn send_address(&mut self, val: Address) -> Result<(), Error> {
        self.send_u64(val.0).await
    }

    /// Read a ROM address when only one device is on the bus
    pub async fn read_rom(&mut self) -> Result<Address, Error> {
        if !self.reset().await? {
            return Err(Error::NoDevicePresent);
        }
        self.send_byte(0x33).await?; // READ ROM command

        let mut address: u64 = 0;
        for i in 0..8 {
            let byte = self.exchange_byte(0xFF).await?;
            address |= (byte as u64) << (i * 8);
        }
        Ok(Address(address))
    }

    /// Read the temperature from a specific DS18B20 sensor
    pub async fn read_temperature(&mut self, address: Address) -> Result<f32, Error> {
        // Start temperature conversion
        if !self.reset().await? {
            return Err(Error::NoDevicePresent);
        }
        self.send_byte(0x55).await?; // MATCH ROM
        self.send_address(address).await?;
        self.send_byte(0x44).await?; // CONVERT T

        // Wait for conversion (750ms max for 12-bit resolution)
        embassy_time::Timer::after(embassy_time::Duration::from_millis(750)).await;

        // Read scratchpad
        if !self.reset().await? {
            return Err(Error::NoDevicePresent);
        }
        self.send_byte(0x55).await?; // MATCH ROM
        self.send_address(address).await?;
        self.send_byte(0xBE).await?; // READ SCRATCHPAD

        // Read 9 bytes of scratchpad
        let mut scratchpad = [0u8; 9];
        for i in 0..9 {
            scratchpad[i] = self.exchange_byte(0xFF).await?;
        }

        // Verify CRC
        if !Self::check_crc(&scratchpad) {
            return Err(Error::CrcError);
        }

        // Convert to temperature
        let raw = i16::from_le_bytes([scratchpad[0], scratchpad[1]]);
        Ok(raw as f32 / 16.0)
    }

    /// Calculate CRC8 for DS18B20
    fn check_crc(data: &[u8]) -> bool {
        let mut crc: u8 = 0;
        for byte in &data[..8] {
            let mut inbyte = *byte;
            for _ in 0..8 {
                let mix = (crc ^ inbyte) & 0x01;
                crc >>= 1;
                if mix != 0 {
                    crc ^= 0x8C;
                }
                inbyte >>= 1;
            }
        }
        crc == data[8]
    }
}

#[derive(Debug)]
pub enum Error {
    InputNotHigh,
    ReceiveTimedOut,
    ReceiveError(esp_hal::rmt::Error),
    SendError(esp_hal::rmt::Error),
    ConfigError(esp_hal::rmt::ConfigError),
    NoDevicePresent,
    CrcError,
}

impl From<esp_hal::rmt::Error> for Error {
    fn from(e: esp_hal::rmt::Error) -> Error {
        Error::SendError(e)
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Address(pub u64);

impl Address {
    /// Get the family code (first byte of address)
    pub fn family_code(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Check if this is a DS18B20 (family code 0x28)
    pub fn is_ds18b20(&self) -> bool {
        self.family_code() == 0x28
    }

    /// Format as standard 1-Wire address string (XX-XXXXXXXXXXXX)
    pub fn to_id_string(&self) -> heapless::String<20> {
        let bytes = self.0.to_le_bytes();
        let mut s = heapless::String::new();
        // Format: family-serial (e.g., 28-00000ab12345)
        let _ = core::fmt::write(
            &mut s,
            format_args!(
                "{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                bytes[0], bytes[6], bytes[5], bytes[4], bytes[3], bytes[2], bytes[1]
            ),
        );
        s
    }
}

impl core::fmt::Debug for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        core::write!(f, "{:X?}", self.0.to_le_bytes())
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        for k in self.0.to_le_bytes() {
            core::write!(f, "{:02X}", k)?;
        }
        Ok(())
    }
}

pub struct Search {
    command: u8,
    address: u64,
    last_discrepancy: Option<usize>,
    complete: bool,
}

#[derive(Debug)]
pub enum SearchError {
    SearchComplete,
    NoDevicesPresent,
    BusError(Error),
}

impl From<Error> for SearchError {
    fn from(e: Error) -> SearchError {
        SearchError::BusError(e)
    }
}

impl Search {
    pub fn new() -> Search {
        Search {
            command: 0xF0,
            address: 0,
            last_discrepancy: None,
            complete: false,
        }
    }

    pub fn new_alarm() -> Search {
        Search {
            command: 0xEC,
            address: 0,
            last_discrepancy: None,
            complete: false,
        }
    }

    /// Check if the search has completed (all devices found)
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub async fn next<'d>(&mut self, ow: &mut OneWire<'d>) -> Result<Address, SearchError> {
        if self.complete {
            return Err(SearchError::SearchComplete);
        }

        let have_devices = ow.reset().await?;
        if !have_devices {
            return Err(SearchError::NoDevicesPresent);
        }

        // Small delay after reset to ensure all devices are ready
        embassy_time::Timer::after(embassy_time::Duration::from_micros(100)).await;

        // Track the last position where we took the 0 path when there was a discrepancy
        // This is where we'll backtrack on the next search iteration
        let mut last_zero_discrepancy: Option<usize> = None;

        ow.send_byte(self.command).await?;

        for id_bit_number in 0..64 {
            // Read two bits: the actual bit value and its complement
            // [0,1] = all devices have 0 at this position
            // [1,0] = all devices have 1 at this position
            // [0,0] = discrepancy - some devices have 0, some have 1
            // [1,1] = no devices responding (error)
            let id_bits = ow.exchange_bits([true, true]).await?;

            let search_direction = match id_bits {
                // All devices have 0 at this bit
                [false, true] => false,

                // All devices have 1 at this bit
                [true, false] => true,

                // No devices responding
                [true, true] => {
                    return Err(SearchError::NoDevicesPresent);
                }

                // Discrepancy: some devices have 0, some have 1
                [false, false] => {
                    if self.last_discrepancy == Some(id_bit_number) {
                        // This is where we backtracked to - now take the 1 path
                        // Don't record as last_zero since we're taking 1
                        true
                    } else if let Some(ld) = self.last_discrepancy {
                        if id_bit_number < ld {
                            // Before our backtrack point - follow previous path
                            let prev_bit = (self.address & (1 << id_bit_number)) != 0;
                            if !prev_bit {
                                // We took 0 path before, record for potential backtrack
                                last_zero_discrepancy = Some(id_bit_number);
                            }
                            prev_bit
                        } else {
                            // After our backtrack point - take 0 path, remember for next time
                            last_zero_discrepancy = Some(id_bit_number);
                            false
                        }
                    } else {
                        // First search, no backtrack point yet - take 0 path
                        log::trace!(
                            "Bit {}: discrepancy, first search, going 0, saving as last_zero",
                            id_bit_number
                        );
                        last_zero_discrepancy = Some(id_bit_number);
                        false
                    }
                }
            };

            // Update the address with this bit
            if search_direction {
                self.address |= 1 << id_bit_number;
            } else {
                self.address &= !(1 << id_bit_number);
            }

            // Tell devices which path we're taking
            ow.exchange_bits([search_direction]).await?;
        }

        // Update state for next iteration
        self.last_discrepancy = last_zero_discrepancy;
        self.complete = last_zero_discrepancy.is_none();

        Ok(Address(self.address))
    }
}
