//! WiFi manager for ESP32-C6
//!
//! Handles WiFi connectivity using esp-radio with Embassy async support.

use alloc::string::String;
use core::str::FromStr;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{ClientConfig, ModeConfig, WifiController};
use log::{info, warn};

/// WiFi credentials and configuration
#[derive(Clone)]
pub struct WifiConfig {
    pub ssid: heapless::String<32>,
    pub password: heapless::String<64>,
}

impl WifiConfig {
    /// Create a new WiFi configuration
    pub fn new(ssid: &str, password: &str) -> Result<Self, &'static str> {
        let ssid = heapless::String::from_str(ssid).map_err(|_| "SSID too long (max 32 chars)")?;
        let password =
            heapless::String::from_str(password).map_err(|_| "Password too long (max 64 chars)")?;

        Ok(Self { ssid, password })
    }
}

/// Initialize WiFi and connect to network
///
/// Returns the network stack for use with MQTT and other network operations.
/// This function spawns background tasks for WiFi connection management.
pub async fn init_wifi(
    spawner: embassy_executor::Spawner,
    config: WifiConfig,
    wifi_peripheral: esp_hal::peripherals::WIFI<'static>,
) -> Result<&'static Stack<'static>, &'static str> {
    info!("Initializing WiFi...");
    info!("SSID: {}", config.ssid);

    // Initialize esp-radio controller (no arguments needed)
    let radio_init = {
        use static_cell::StaticCell;
        static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
        RADIO.init_with(|| esp_radio::init().expect("Failed to initialize radio controller"))
    };

    // Create WiFi controller and interfaces
    let (wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_init, wifi_peripheral, Default::default())
            .map_err(|_| "Failed to create WiFi device")?;

    let wifi_interface = interfaces.sta;

    // Generate random seed for network stack
    let seed = 1234u64; // In production, use hardware RNG

    // Create network stack resources (static allocation)
    let resources = {
        use static_cell::StaticCell;
        static RESOURCES: StaticCell<embassy_net::StackResources<3>> = StaticCell::new();
        RESOURCES.init_with(|| embassy_net::StackResources::new())
    };

    // Create network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        embassy_net::Config::dhcpv4(Default::default()),
        resources,
        seed,
    );

    let stack = {
        use static_cell::StaticCell;
        static STACK: StaticCell<Stack<'static>> = StaticCell::new();
        STACK.init_with(|| stack)
    };

    // Spawn network stack task
    spawner
        .spawn(connection_task(runner))
        .map_err(|_| "Failed to spawn network task")?;

    // Spawn WiFi controller task
    spawner
        .spawn(wifi_task(wifi_controller, config))
        .map_err(|_| "Failed to spawn WiFi task")?;

    Ok(stack)
}

/// Embassy task for network stack
#[embassy_executor::task]
async fn connection_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
) {
    runner.run().await
}

/// Embassy task for WiFi connection management
#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>, config: WifiConfig) {
    info!("WiFi task starting");

    loop {
        // Wait for WiFi driver to be ready
        if !controller.is_started().unwrap_or(false) {
            let client_config = ModeConfig::Client(
                ClientConfig::default()
                    .with_ssid(String::from(config.ssid.as_str()))
                    .with_password(String::from(config.password.as_str())),
            );

            if let Err(e) = controller.set_config(&client_config) {
                warn!("Failed to set WiFi configuration: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }

            info!("Starting WiFi controller");
            if let Err(e) = controller.start() {
                warn!("Failed to start WiFi: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        }

        // Try to connect (not async in esp-radio)
        match controller.connect() {
            Ok(_) => {
                info!("WiFi connected successfully");

                // Keep running - connection is maintained by controller
                loop {
                    Timer::after(Duration::from_secs(60)).await;
                }
            }
            Err(e) => {
                warn!("WiFi connection failed: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wifi_config_creation() {
        let config = WifiConfig::new("MyNetwork", "password123").unwrap();
        assert_eq!(config.ssid.as_str(), "MyNetwork");
        assert_eq!(config.password.as_str(), "password123");
    }

    #[test]
    fn test_wifi_config_ssid_too_long() {
        let long_ssid = "ThisSSIDIsWayTooLongForTheMaximumAllowedLength";
        assert!(WifiConfig::new(long_ssid, "password").is_err());
    }

    #[test]
    fn test_wifi_config_password_too_long() {
        let long_password = "ThisPasswordIsWayTooLongForTheMaximumAllowedLengthAndShouldBeRejected";
        assert!(WifiConfig::new("MyNetwork", long_password).is_err());
    }
}
