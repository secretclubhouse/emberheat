//! WiFi manager for ESP32-C6
//!
//! Handles WiFi connectivity using esp-radio with Embassy async support.

use core::str::FromStr;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, Password, Ssid, WifiController,
};
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

    // Create the WiFi controller. esp-radio 1.0 initializes the radio itself;
    // there is no separate radio controller object any more.
    let wifi_controller = WifiController::new(wifi_peripheral, ControllerConfig::default())
        .map_err(|_| "Failed to create WiFi device")?;

    // The station (client) interface is the embassy-net driver
    let wifi_interface = Interface::station();

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
    spawner.spawn(connection_task(runner).map_err(|_| "Failed to spawn network task")?);

    // Spawn WiFi controller task
    spawner.spawn(wifi_task(wifi_controller, config).map_err(|_| "Failed to spawn WiFi task")?);

    Ok(stack)
}

/// Embassy task for network stack
#[embassy_executor::task]
async fn connection_task(mut runner: embassy_net::Runner<'static, Interface>) {
    runner.run().await
}

/// Embassy task for WiFi connection management
#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>, config: WifiConfig) {
    info!("WiFi task starting");

    loop {
        let station_config = match build_station_config(&config) {
            Ok(station_config) => station_config,
            Err(e) => {
                warn!("Invalid WiFi configuration: {}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Err(e) = controller.set_config(&Config::Station(station_config)) {
            warn!("Failed to set WiFi configuration: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        match controller.connect_async().await {
            Ok(_) => {
                info!("WiFi connected successfully");
                // Stay here while the connection is up, then loop to reconnect
                let _ = controller.wait_for_disconnect_async().await;
                warn!("WiFi disconnected, reconnecting");
            }
            Err(e) => {
                warn!("WiFi connection failed: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Build the esp-radio station configuration from our WiFi settings
fn build_station_config(config: &WifiConfig) -> Result<StationConfig, &'static str> {
    let ssid = Ssid::try_from(config.ssid.as_str()).map_err(|_| "invalid SSID")?;
    let authentication = if config.password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(
            Password::try_from(config.password.as_str()).map_err(|_| "invalid password")?,
        )
    };

    Ok(StationConfig::default()
        .with_ssid(ssid)
        .with_authentication(authentication))
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
