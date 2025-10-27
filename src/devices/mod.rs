//! Thermostat device implementation
//!
//! This module implements the core thermostat logic with hysteresis-based control.
//! Each thermostat controls a single heating zone and manages:
//!
//! - Temperature sensing (1-Wire or HTTP)
//! - Relay control (heating on/off)
//! - Hysteresis logic to prevent rapid cycling
//! - State persistence
//! - Metrics collection
//! - Sensor error recovery (power cycling)
//!
//! # Hysteresis Logic
//!
//! The thermostat uses a dead band of ±0.5°C to prevent rapid on/off cycling:
//!
//! - Turn ON when: temp <= target - 0.5°C
//! - Turn OFF when: temp >= target + 0.5°C
//! - Stay in current state when: within ±0.5°C band
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::devices::Thermostat;
//! use thermos_rust::config::ZoneConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = ZoneConfig {
//!         relay_pin: 23,
//!         temp_pin: 4,
//!         temp_id: "00000ab12345".to_string(),
//!         extra_sensor: None,
//!     };
//!
//!     let thermostat = Thermostat::new("Zone1", config).await?;
//!     thermostat.run().await?;
//!     Ok(())
//! }
//! ```

use crate::config::{Config, ZoneConfig};
use crate::gpio::{GpioController, RelayState};
use crate::metrics;
use crate::persistence::{StateManager, ThermostatState};
use crate::sensors::{CompositeSensor, TemperatureReading, TemperatureSensor};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

/// Hysteresis upper threshold (°C)
/// Turn OFF when temp >= target + this value
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;

/// Hysteresis lower threshold (°C)
/// Turn ON when temp <= target + this value
const HYSTERESIS_LOWER_THRESHOLD_C: f32 = -0.5;

/// Minimum valid temperature in Celsius
const SENSOR_ERROR_MIN_TEMP_C: f32 = 3.0;

/// Maximum valid temperature in Celsius
const SENSOR_ERROR_MAX_TEMP_C: f32 = 37.0;

/// Default temperature on error (°C)
const TEMP_ERROR_DEFAULT_C: f32 = 21.1;

/// Default target temperature (°C)
const DEFAULT_TARGET_TEMP_C: f32 = 21.1;

/// Control loop interval (seconds)
const CONTROL_LOOP_INTERVAL_SECS: u64 = 6;

/// VCC power cycle down time (seconds)
const POWER_CYCLE_DOWN_TIME_SECS: u64 = 3;

/// VCC power cycle recovery time (seconds)
const POWER_CYCLE_RECOVERY_TIME_SECS: u64 = 5;

/// VCC GPIO pin (shared across all sensors)
const VCC_PIN: u8 = 0;

/// Thermostat state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatingState {
    /// Heating is off
    Off = 0,
    /// Heating is on
    Heat = 1,
}

impl From<u8> for HeatingState {
    fn from(value: u8) -> Self {
        match value {
            0 => HeatingState::Off,
            _ => HeatingState::Heat,
        }
    }
}

impl From<HeatingState> for u8 {
    fn from(state: HeatingState) -> Self {
        state as u8
    }
}

impl From<HeatingState> for RelayState {
    fn from(state: HeatingState) -> Self {
        match state {
            HeatingState::Off => RelayState::Off,
            HeatingState::Heat => RelayState::On,
        }
    }
}

/// Thermostat implementation
pub struct Thermostat {
    /// Zone name
    name: String,

    /// Zone configuration
    config: ZoneConfig,

    /// Temperature sensor
    sensor: CompositeSensor,

    /// GPIO controller
    gpio: Arc<GpioController>,

    /// State persistence
    state_manager: Arc<StateManager>,

    /// Current temperature (Celsius)
    current_temp: Arc<RwLock<f32>>,

    /// Target temperature (Celsius)
    target_temp: Arc<RwLock<f32>>,

    /// Target heating state
    target_state: Arc<RwLock<HeatingState>>,

    /// Current heating state
    current_state: Arc<RwLock<HeatingState>>,

    /// Sensor reset tracking
    sensor_being_reset: Arc<RwLock<HashMap<String, Instant>>>,
}

impl Thermostat {
    /// Create a new thermostat
    ///
    /// # Arguments
    ///
    /// * `name` - Zone name
    /// * `config` - Zone configuration
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New thermostat instance or error
    pub async fn new(
        name: &str,
        config: ZoneConfig,
        gpio: Arc<GpioController>,
        state_manager: Arc<StateManager>,
    ) -> Result<Self> {
        info!("Initializing thermostat for zone: {}", name);

        // Initialize sensor
        let sensor = CompositeSensor::from_onewire(&config.temp_id, config.extra_sensor.clone())
            .context("Failed to initialize temperature sensor")?;

        // Initialize GPIO relay
        gpio.init_relay(config.relay_pin)
            .context("Failed to initialize relay")?;

        // Load or create initial state
        let initial_state =
            state_manager
                .load_state(name)
                .await?
                .unwrap_or_else(|| ThermostatState {
                    relay_pin: config.relay_pin,
                    temp_pin: config.temp_pin,
                    temp_id: config.temp_id.clone(),
                    target_temp: DEFAULT_TARGET_TEMP_C,
                    target_state: HeatingState::Off.into(),
                    current_state: HeatingState::Off.into(),
                    last_updated: chrono::Utc::now(),
                });

        info!(
            "Zone {} initial state - target: {:.1}°C, target_state: {:?}, current_state: {:?}",
            name,
            initial_state.target_temp,
            HeatingState::from(initial_state.target_state),
            HeatingState::from(initial_state.current_state)
        );

        Ok(Self {
            name: name.to_string(),
            config,
            sensor,
            gpio,
            state_manager,
            current_temp: Arc::new(RwLock::new(TEMP_ERROR_DEFAULT_C)),
            target_temp: Arc::new(RwLock::new(initial_state.target_temp)),
            target_state: Arc::new(RwLock::new(HeatingState::from(initial_state.target_state))),
            current_state: Arc::new(RwLock::new(HeatingState::from(initial_state.current_state))),
            sensor_being_reset: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get current temperature (Celsius)
    pub async fn get_current_temp(&self) -> f32 {
        *self.current_temp.read().await
    }

    /// Get target temperature (Celsius)
    pub async fn get_target_temp(&self) -> f32 {
        *self.target_temp.read().await
    }

    /// Set target temperature (Celsius)
    pub async fn set_target_temp(&self, temp: f32) -> Result<()> {
        info!(
            "Zone {}: Setting target temperature to {:.1}°C",
            self.name, temp
        );

        *self.target_temp.write().await = temp;

        // Persist state
        self.save_state().await?;

        Ok(())
    }

    /// Get target heating state
    pub async fn get_target_state(&self) -> HeatingState {
        *self.target_state.read().await
    }

    /// Set target heating state
    pub async fn set_target_state(&self, state: HeatingState) -> Result<()> {
        info!("Zone {}: Setting target state to {:?}", self.name, state);

        *self.target_state.write().await = state;

        // Persist state
        self.save_state().await?;

        Ok(())
    }

    /// Get current heating state
    pub async fn get_current_state(&self) -> HeatingState {
        *self.current_state.read().await
    }

    /// Save current state to persistence
    async fn save_state(&self) -> Result<()> {
        let state = ThermostatState {
            relay_pin: self.config.relay_pin,
            temp_pin: self.config.temp_pin,
            temp_id: self.config.temp_id.clone(),
            target_temp: *self.target_temp.read().await,
            target_state: (*self.target_state.read().await).into(),
            current_state: (*self.current_state.read().await).into(),
            last_updated: chrono::Utc::now(),
        };

        self.state_manager.save_state(&self.name, &state).await
    }

    /// Read temperature from sensor
    async fn read_temperature(&self) -> Result<TemperatureReading> {
        let start = Instant::now();

        let reading = self.sensor.read_temperature().await?;

        let duration = start.elapsed();
        metrics::update_response_time(&self.name, duration);

        Ok(reading)
    }

    /// Handle sensor error with power cycling
    async fn handle_sensor_error(&self, error: &anyhow::Error) {
        warn!("Zone {}: Sensor error: {}", self.name, error);

        let sensor_id = self.sensor.id();

        // Check if already resetting
        {
            let reset_map = self.sensor_being_reset.read().await;
            if let Some(reset_time) = reset_map.get(sensor_id) {
                let elapsed = reset_time.elapsed();
                if elapsed
                    < Duration::from_secs(
                        POWER_CYCLE_DOWN_TIME_SECS + POWER_CYCLE_RECOVERY_TIME_SECS,
                    )
                {
                    debug!(
                        "Zone {}: Sensor {} already being reset, skipping (elapsed: {:?})",
                        self.name, sensor_id, elapsed
                    );
                    return;
                }
            }
        }

        // Mark sensor as being reset
        {
            let mut reset_map = self.sensor_being_reset.write().await;
            reset_map.insert(sensor_id.to_string(), Instant::now());
        }

        info!("Zone {}: Initiating sensor power cycle", self.name);
        metrics::increment_reset_error_count(&self.name);

        // Power cycle VCC
        if let Err(e) = self
            .gpio
            .power_cycle_vcc(Duration::from_secs(POWER_CYCLE_DOWN_TIME_SECS))
            .await
        {
            error!("Zone {}: Failed to power cycle VCC: {}", self.name, e);
        }

        // Wait for recovery
        sleep(Duration::from_secs(POWER_CYCLE_RECOVERY_TIME_SECS)).await;

        // Remove from reset tracking
        {
            let mut reset_map = self.sensor_being_reset.write().await;
            reset_map.remove(sensor_id);
        }

        info!("Zone {}: Sensor power cycle complete", self.name);
    }

    /// Apply hysteresis control logic
    async fn apply_control_logic(&self, current_temp_c: f32) {
        let target_temp_c = *self.target_temp.read().await;
        let target_state = *self.target_state.read().await;
        let current_relay_state = self
            .gpio
            .get_relay_state(self.config.relay_pin)
            .unwrap_or(RelayState::Off);

        let new_relay_state = if target_state == HeatingState::Heat {
            // Hysteresis logic
            let lower_bound = target_temp_c + HYSTERESIS_LOWER_THRESHOLD_C;
            let upper_bound = target_temp_c + HYSTERESIS_UPPER_THRESHOLD_C;

            if current_temp_c <= lower_bound {
                RelayState::On
            } else if current_temp_c >= upper_bound {
                RelayState::Off
            } else {
                // Within dead band - maintain current state
                current_relay_state
            }
        } else {
            // Target is OFF - force relay off
            RelayState::Off
        };

        // Update relay if state changed
        if new_relay_state != current_relay_state {
            if let Err(e) = self.gpio.set_relay(self.config.relay_pin, new_relay_state) {
                error!("Zone {}: Failed to set relay: {}", self.name, e);
            } else {
                let new_heating_state = match new_relay_state {
                    RelayState::On => HeatingState::Heat,
                    RelayState::Off => HeatingState::Off,
                };

                info!(
                    "Zone {}: Relay state changed to {:?} (temp: {:.1}°C, target: {:.1}°C)",
                    self.name, new_heating_state, current_temp_c, target_temp_c
                );

                *self.current_state.write().await = new_heating_state;

                // Persist state change
                if let Err(e) = self.save_state().await {
                    error!("Zone {}: Failed to save state: {}", self.name, e);
                }

                // Update metrics
                metrics::update_heat_status(&self.name, new_heating_state == HeatingState::Heat);
            }
        }
    }

    /// Update metrics
    async fn update_metrics(&self, temp_reading: &TemperatureReading) {
        let heat_on = *self.current_state.read().await == HeatingState::Heat;
        let target_temp = *self.target_temp.read().await;

        metrics::update_current_temp(&self.name, temp_reading.fahrenheit, heat_on);
        metrics::update_target_temp(&self.name, target_temp * 9.0 / 5.0 + 32.0, heat_on);
        metrics::update_heat_status(&self.name, heat_on);
    }

    /// Main control loop
    ///
    /// Runs continuously, reading temperature and applying control logic
    pub async fn run(&self) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(CONTROL_LOOP_INTERVAL_SECS));

        info!("Zone {}: Starting control loop", self.name);

        loop {
            ticker.tick().await;

            // Read temperature
            match self.read_temperature().await {
                Ok(reading) => {
                    debug!(
                        "Zone {}: Temperature reading: {:.1}°C / {:.1}°F",
                        self.name, reading.celsius, reading.fahrenheit
                    );

                    *self.current_temp.write().await = reading.celsius;

                    // Apply control logic
                    self.apply_control_logic(reading.celsius).await;

                    // Update metrics
                    self.update_metrics(&reading).await;
                }
                Err(e) => {
                    error!("Zone {}: Failed to read temperature: {}", self.name, e);

                    // Handle sensor error
                    self.handle_sensor_error(&e).await;

                    // Use default temperature
                    *self.current_temp.write().await = TEMP_ERROR_DEFAULT_C;
                }
            }
        }
    }

    /// Get zone name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Thermostat bridge managing multiple zones
pub struct ThermostatBridge {
    thermostats: HashMap<String, Arc<Thermostat>>,
    gpio: Arc<GpioController>,
}

impl ThermostatBridge {
    /// Create a new thermostat bridge
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration with all zones
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New bridge instance or error
    pub async fn new(config: Config) -> Result<Self> {
        info!(
            "Creating thermostat bridge with {} zones",
            config.zones.len()
        );

        // Initialize GPIO controller
        let gpio = Arc::new(GpioController::new()?);

        // Initialize VCC controller
        gpio.init_vcc_controller(VCC_PIN)?;

        // Initialize state manager with Redis URL from config (or default)
        let redis_url = config.redis_url();
        info!("Connecting to Redis at {}", redis_url);
        let state_manager = Arc::new(StateManager::new(redis_url).await?);

        // Create thermostats for each zone
        let mut thermostats = HashMap::new();

        for (zone_name, zone_config) in config.zones {
            let thermostat = Arc::new(
                Thermostat::new(&zone_name, zone_config, gpio.clone(), state_manager.clone())
                    .await?,
            );

            thermostats.insert(zone_name, thermostat);
        }

        info!("Thermostat bridge created successfully");

        Ok(Self { thermostats, gpio })
    }

    /// Get all thermostats
    pub fn thermostats(&self) -> &HashMap<String, Arc<Thermostat>> {
        &self.thermostats
    }

    /// Get a specific thermostat
    pub fn get_thermostat(&self, name: &str) -> Option<Arc<Thermostat>> {
        self.thermostats.get(name).cloned()
    }

    /// Run all thermostats
    pub async fn run(&self) -> Result<()> {
        info!("Starting all thermostat control loops");

        let mut handles = Vec::new();

        for (zone_name, thermostat) in &self.thermostats {
            let thermostat = thermostat.clone();
            let zone_name = zone_name.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = thermostat.run().await {
                    error!("Zone {} control loop error: {}", zone_name, e);
                }
            });

            handles.push(handle);
        }

        // Wait for all control loops (should run indefinitely)
        for handle in handles {
            handle.await?;
        }

        Ok(())
    }
}

impl Drop for ThermostatBridge {
    fn drop(&mut self) {
        info!("Shutting down thermostat bridge");
        self.gpio.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heating_state_conversion() {
        assert_eq!(HeatingState::from(0), HeatingState::Off);
        assert_eq!(HeatingState::from(1), HeatingState::Heat);
        assert_eq!(u8::from(HeatingState::Off), 0);
        assert_eq!(u8::from(HeatingState::Heat), 1);
    }

    #[test]
    fn test_constants() {
        assert_eq!(HYSTERESIS_UPPER_THRESHOLD_C, 0.5);
        assert_eq!(HYSTERESIS_LOWER_THRESHOLD_C, -0.5);
        assert_eq!(DEFAULT_TARGET_TEMP_C, 21.1);
        assert_eq!(CONTROL_LOOP_INTERVAL_SECS, 6);
    }

    // Integration tests would require mocking GPIO, sensors, and Redis
}
