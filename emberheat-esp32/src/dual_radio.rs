//! Dual-radio driver for ESP32-C6 (WiFi) + ESP32-H2 (BLE over UART HCI)
//!
//! This module provides `DualRadioDriver` which implements rs-matter-embassy's
//! `WifiCoexDriver` trait by:
//! - Using ESP32-C6's internal WiFi via esp-radio
//! - Communicating with ESP32-H2's BLE controller over UART HCI (H4 protocol)

use bt_hci::controller::ExternalController;
use bt_hci::transport::{PacketKind, PacketToController, PacketToHost, Transport, WithIndicator};
use bt_hci::ReadHciError;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;

use embedded_io_async::{ErrorType, Read, ReadExactError, Write};

use esp_hal::uart::{RxError, TxError, UartRx, UartTx};
use esp_hal::Async;

use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::wifi::esp::EspWifiController;
use rs_matter_embassy::wireless::{
    BleDriver, BleDriverTask, WifiCoexDriver, WifiCoexDriverTask, WifiDriver, WifiDriverTask,
};

use log::{info, warn};

/// Number of HCI command slots for the BLE controller
///
/// This matches the internal SLOTS constant in rs-matter-embassy.
const SLOTS: usize = 20;

/// Unified UART error type for HCI transport
///
/// The bt-hci `SerialTransport` requires both reader and writer to have the same
/// error type. This enum unifies `RxError` and `TxError` from esp-hal.
#[derive(Debug, Clone, Copy)]
pub enum UartHciError {
    /// UART receive error
    Rx(RxError),
    /// UART transmit error (currently infallible in esp-hal)
    Tx(TxError),
}

impl core::fmt::Display for UartHciError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl core::error::Error for UartHciError {}

impl embedded_io_async::Error for UartHciError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        embedded_io_async::ErrorKind::Other
    }
}

impl From<RxError> for UartHciError {
    fn from(e: RxError) -> Self {
        Self::Rx(e)
    }
}

impl From<TxError> for UartHciError {
    fn from(e: TxError) -> Self {
        Self::Tx(e)
    }
}

/// Wrapper for UartRx that converts RxError to UartHciError
///
/// This adapts esp-hal's UartRx (which implements embedded-io 0.7 traits)
/// to the embedded-io 0.6 traits expected by bt-hci.
pub struct HciUartRx<'d>(pub UartRx<'d, Async>);

impl ErrorType for HciUartRx<'_> {
    type Error = UartHciError;
}

impl Read for HciUartRx<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Use esp-hal's async read and convert the error
        esp_hal::uart::UartRx::read_async(&mut self.0, buf)
            .await
            .map_err(UartHciError::from)
    }
}

/// Wrapper for UartTx that converts TxError to UartHciError
///
/// This adapts esp-hal's UartTx (which implements embedded-io 0.7 traits)
/// to the embedded-io 0.6 traits expected by bt-hci.
pub struct HciUartTx<'d>(pub UartTx<'d, Async>);

impl ErrorType for HciUartTx<'_> {
    type Error = UartHciError;
}

impl Write for HciUartTx<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        // Use esp-hal's async write and convert the error
        esp_hal::uart::UartTx::write_async(&mut self.0, buf)
            .await
            .map_err(UartHciError::from)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        // Use esp-hal's async flush and convert the error
        esp_hal::uart::UartTx::flush_async(&mut self.0)
            .await
            .map_err(UartHciError::from)
    }
}

/// Errors from the UART HCI transport
#[derive(Debug, Clone, Copy)]
pub enum HciTransportError {
    /// The UART reported an error
    Uart(UartHciError),
    /// The UART closed, or the H2 sent bytes that are not a valid HCI packet
    InvalidPacket,
}

impl core::fmt::Display for HciTransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl core::error::Error for HciTransportError {}

impl embedded_io_async::Error for HciTransportError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        embedded_io_async::ErrorKind::Other
    }
}

impl From<ReadHciError<UartHciError>> for HciTransportError {
    fn from(e: ReadHciError<UartHciError>) -> Self {
        match e {
            ReadHciError::Read(ReadExactError::Other(e)) => Self::Uart(e),
            _ => Self::InvalidPacket,
        }
    }
}

// Required by bt-hci's `ExternalController` for packets it parses from memory
impl From<ReadHciError<core::convert::Infallible>> for HciTransportError {
    fn from(_: ReadHciError<core::convert::Infallible>) -> Self {
        Self::InvalidPacket
    }
}

/// HCI over UART (H4 framing: one packet-type byte, then the packet).
///
/// bt-hci 0.10 removed its built-in `SerialTransport`; this is the local equivalent.
pub struct SerialTransport<R, W> {
    reader: Mutex<NoopRawMutex, R>,
    writer: Mutex<NoopRawMutex, W>,
}

impl<R, W> SerialTransport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: Mutex::new(reader),
            writer: Mutex::new(writer),
        }
    }
}

impl<R, W> ErrorType for SerialTransport<R, W> {
    type Error = HciTransportError;
}

impl<R, W> Transport for SerialTransport<R, W>
where
    R: Read<Error = UartHciError>,
    W: Write<Error = UartHciError>,
{
    async fn read<'a, P: PacketToHost<'a>>(&self, rx: &'a mut [u8]) -> Result<P, Self::Error> {
        let mut reader = self.reader.lock().await;
        let kind = PacketKind::read_async(&mut *reader).await?;
        Ok(P::read_hci_async(kind, &mut *reader, rx).await?)
    }

    async fn write<P: PacketToController>(&self, tx: &P) -> Result<(), Self::Error> {
        let mut writer = self.writer.lock().await;
        WithIndicator::new(tx)
            .write_hci_async(&mut *writer)
            .await
            .map_err(HciTransportError::Uart)
    }
}

/// Dual-radio WiFi+BLE driver
///
/// Uses ESP32-C6's internal WiFi and ESP32-H2's BLE over UART HCI.
pub struct DualRadioDriver<'d> {
    wifi_peripheral: esp_hal::peripherals::WIFI<'d>,
    uart_rx: HciUartRx<'d>,
    uart_tx: HciUartTx<'d>,
}

impl<'d> DualRadioDriver<'d> {
    /// Create a new dual-radio driver
    ///
    /// # Arguments
    /// - `wifi_peripheral` - The WiFi peripheral
    /// - `uart_rx` - UART RX for HCI from ESP32-H2
    /// - `uart_tx` - UART TX for HCI to ESP32-H2
    pub fn new(
        wifi_peripheral: esp_hal::peripherals::WIFI<'d>,
        uart_rx: UartRx<'d, Async>,
        uart_tx: UartTx<'d, Async>,
    ) -> Self {
        Self {
            wifi_peripheral,
            uart_rx: HciUartRx(uart_rx),
            uart_tx: HciUartTx(uart_tx),
        }
    }
}

impl DualRadioDriver<'_> {
    /// Check the link to the freshly reset H2 and clear the receive side.
    ///
    /// `main` pulses the H2's reset line before this runs, so the H2's HCI
    /// parser starts at a packet boundary. Anything the C6 received while the
    /// H2 was resetting is noise and would throw the C6's own parser out of
    /// step, so it is discarded here.
    async fn prepare_link(&mut self) {
        use embassy_futures::select::{select, Either};
        use embassy_time::{Duration, Timer};

        let mut buf = [0u8; 64];
        let mut discarded = 0usize;
        while let Either::First(result) = select(
            self.uart_rx.read(&mut buf),
            Timer::after(Duration::from_millis(100)),
        )
        .await
        {
            match result {
                Ok(n) => discarded += n,
                Err(e) => warn!("DualRadioDriver: HCI read error while clearing: {:?}", e),
            }
        }
        if discarded > 0 {
            info!(
                "DualRadioDriver: discarded {} stray bytes from H2",
                discarded
            );
        }

        // A running H2 drives its TX (our RX) high and overrides the weak
        // internal pull-down; an unpowered or disconnected one does not.
        let mux = esp_hal::peripherals::IO_MUX::regs();
        let rx_pin = crate::config::HCI_UART_RX_PIN as usize;
        mux.gpio(rx_pin)
            .modify(|_, w| w.fun_wpd().set_bit().fun_wpu().clear_bit());
        Timer::after(Duration::from_millis(20)).await;
        let driven = (esp_hal::peripherals::GPIO::regs().in_().read().bits() >> rx_pin) & 1 == 1;
        mux.gpio(rx_pin)
            .modify(|_, w| w.fun_wpd().clear_bit().fun_wpu().set_bit());
        if driven {
            info!("DualRadioDriver: H2 link is up");
        } else {
            warn!(
                "DualRadioDriver: nothing is driving the H2 TX line (GPIO{}); the H2 is \
                 unpowered, held in reset or a wire is loose. Bluetooth will not start.",
                rx_pin
            );
        }
    }
}

impl WifiDriver for DualRadioDriver<'_> {
    type NetCtl<'a>
        = EspWifiController<'a>
    where
        Self: 'a;

    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: WifiDriverTask,
    {
        let mut controller = esp_radio::wifi::WifiController::new(
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::ControllerConfig::default(),
        )
        .expect("Failed to create WiFi interface");

        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable power saving");

        // esp-radio 1.0 lowers the default max TX power to 5dBm, which makes
        // scans and connections unreliable on the C6. Restore 20dBm, as the
        // upstream rs-matter-embassy ESP driver does.
        controller
            .set_max_tx_power(80)
            .expect("Failed to set WiFi TX power");

        task.run(
            esp_radio::wifi::Interface::station(),
            EspWifiController::new(controller),
        )
        .await
    }
}

impl WifiCoexDriver for DualRadioDriver<'_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: WifiCoexDriverTask,
    {
        info!("DualRadioDriver: Initializing UART HCI transport to ESP32-H2");

        self.prepare_link().await;

        // Create BLE controller from UART transport
        let transport = SerialTransport::new(&mut self.uart_rx, &mut self.uart_tx);
        let ble_ctl: ExternalController<_, SLOTS> = ExternalController::new(transport);

        info!("DualRadioDriver: BLE controller ready, initializing WiFi");

        // Initialize WiFi
        let mut controller = esp_radio::wifi::WifiController::new(
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::ControllerConfig::default(),
        )
        .expect("Failed to create WiFi interface");

        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable power saving");

        // esp-radio 1.0 lowers the default max TX power to 5dBm, which makes
        // scans and connections unreliable on the C6. Restore 20dBm, as the
        // upstream rs-matter-embassy ESP driver does.
        controller
            .set_max_tx_power(80)
            .expect("Failed to set WiFi TX power");

        info!("DualRadioDriver: Running coex task with dual radios");

        task.run(
            esp_radio::wifi::Interface::station(),
            EspWifiController::new(controller),
            ble_ctl,
        )
        .await
    }
}

impl BleDriver for DualRadioDriver<'_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: BleDriverTask,
    {
        info!("DualRadioDriver: Running BLE-only task via UART HCI");

        self.prepare_link().await;

        let transport = SerialTransport::new(&mut self.uart_rx, &mut self.uart_tx);
        let ble_ctl: ExternalController<_, SLOTS> = ExternalController::new(transport);

        task.run(ble_ctl).await
    }
}
