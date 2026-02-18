//! Dual-radio driver for ESP32-C6 (WiFi) + ESP32-H2 (BLE over UART HCI)
//!
//! This module provides `DualRadioDriver` which implements rs-matter-embassy's
//! `WifiCoexDriver` trait by:
//! - Using ESP32-C6's internal WiFi via esp-radio
//! - Communicating with ESP32-H2's BLE controller over UART HCI (H4 protocol)

use bt_hci::controller::ExternalController;
use bt_hci::transport::SerialTransport;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;

// Use embedded_io_async 0.6 which matches bt-hci's requirements
use embedded_io_async::{ErrorType, Read, Write};

use esp_hal::uart::{RxError, TxError, UartRx, UartTx};
use esp_hal::Async;

use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::wifi::esp::EspWifiController;
use rs_matter_embassy::wireless::{
    BleDriver, BleDriverTask, WifiCoexDriver, WifiCoexDriverTask, WifiDriver, WifiDriverTask,
};

use log::info;

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

/// Dual-radio WiFi+BLE driver
///
/// Uses ESP32-C6's internal WiFi and ESP32-H2's BLE over UART HCI.
pub struct DualRadioDriver<'a, 'd> {
    controller: &'a esp_radio::Controller<'d>,
    wifi_peripheral: esp_hal::peripherals::WIFI<'d>,
    uart_rx: HciUartRx<'d>,
    uart_tx: HciUartTx<'d>,
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
            uart_rx: HciUartRx(uart_rx),
            uart_tx: HciUartTx(uart_tx),
        }
    }
}

impl WifiDriver for DualRadioDriver<'_, '_> {
    async fn run<A>(&mut self, mut task: A) -> Result<(), Error>
    where
        A: WifiDriverTask,
    {
        let (mut controller, wifi_interface) = esp_radio::wifi::new(
            self.controller,
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::Config::default(),
        )
        .expect("Failed to create WiFi interface");

        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable power saving");

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
        let (mut controller, wifi_interface) = esp_radio::wifi::new(
            self.controller,
            self.wifi_peripheral.reborrow(),
            esp_radio::wifi::Config::default(),
        )
        .expect("Failed to create WiFi interface");

        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable power saving");

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
