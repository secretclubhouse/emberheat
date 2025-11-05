//! Matter protocol integration for HomeKit compatibility
//!
//! This module provides Matter protocol support for integrating thermostats with
//! Apple HomeKit and other Matter-compatible smart home ecosystems via the
//! Connected Home IP library.
//!
//! Matter (formerly Project CHIP) is an open-source, royalty-free connectivity
//! standard for smart home devices. It enables interoperability between devices
//! from different manufacturers and ecosystems.
//!
//! # Architecture
//!
//! ```text
//! HomeKit/Google Home/Alexa
//!         │
//!         ↓
//!   Matter Protocol (Connected Home IP)
//!         │
//!         ↓
//!   FFI Wrapper (C/C++ ↔ Rust)
//!         │
//!         ↓
//!   MatterServer
//!         │
//!         ↓
//!   ThermostatAccessory (per zone)
//!         │
//!         ↓
//!   Thermostat Device
//! ```
//!
//! # Matter Clusters
//!
//! The thermostat implementation uses these Matter clusters:
//! - **Basic Information** (0x0028) - Device metadata
//! - **Thermostat** (0x0201) - Thermostat functionality
//!   - LocalTemperature
//!   - OccupiedCoolingSetpoint
//!   - OccupiedHeatingSetpoint
//!   - SystemMode
//!   - ControlSequenceOfOperation
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::matter::MatterServer;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut server = MatterServer::new(5540).await?;
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

// FFI module for Connected Home IP integration
pub mod ffi;

use crate::devices::Thermostat;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Matter server port (standard Matter port is 5540)
const MATTER_PORT: u16 = 5540;

/// Matter vendor ID (placeholder - should be registered)
const VENDOR_ID: u16 = 0xFFF1;

/// Matter product ID
const PRODUCT_ID: u16 = 0x8001;

/// Device type: Thermostat (0x0301)
const DEVICE_TYPE_THERMOSTAT: u16 = 0x0301;

/// Thermostat cluster ID (0x0201)
const CLUSTER_THERMOSTAT: u16 = 0x0201;

/// Basic information cluster ID (0x0028)
const CLUSTER_BASIC_INFO: u16 = 0x0028;

/// Matter thermostat accessory
///
/// Wraps a Thermostat instance and exposes it via Matter protocol
pub struct ThermostatAccessory {
    /// Zone name / accessory name
    name: String,

    /// Underlying thermostat device
    thermostat: Arc<Thermostat>,

    /// Matter FFI thermostat handle
    matter_thermostat: Option<Arc<RwLock<ffi::Thermostat>>>,

    /// Endpoint ID in Matter fabric
    endpoint_id: u16,
}

impl ThermostatAccessory {
    /// Create a new Matter thermostat accessory
    ///
    /// # Arguments
    ///
    /// * `name` - Accessory name (zone name)
    /// * `thermostat` - Thermostat device instance
    /// * `endpoint_id` - Matter endpoint ID
    pub fn new(name: String, thermostat: Arc<Thermostat>, endpoint_id: u16) -> Self {
        info!(
            "Creating Matter thermostat accessory '{}' on endpoint {}",
            name, endpoint_id
        );

        Self {
            name,
            thermostat,
            matter_thermostat: None,
            endpoint_id,
        }
    }

    /// Set the Matter FFI thermostat handle
    pub(crate) fn set_matter_thermostat(&mut self, matter_thermostat: Arc<RwLock<ffi::Thermostat>>) {
        self.matter_thermostat = Some(matter_thermostat);
    }

    /// Get current temperature in centi-degrees Celsius (Matter format)
    ///
    /// Matter represents temperatures as int16 in hundredths of degrees Celsius
    pub async fn get_local_temperature(&self) -> i16 {
        let temp_c = self.thermostat.get_current_temp().await;
        (temp_c * 100.0) as i16
    }

    /// Get occupied heating setpoint in centi-degrees Celsius
    pub async fn get_occupied_heating_setpoint(&self) -> i16 {
        let temp_c = self.thermostat.get_target_temp().await;
        (temp_c * 100.0) as i16
    }

    /// Set occupied heating setpoint
    ///
    /// # Arguments
    ///
    /// * `value` - Temperature in centi-degrees Celsius
    pub async fn set_occupied_heating_setpoint(&self, value: i16) -> Result<()> {
        let temp_c = value as f32 / 100.0;
        self.thermostat.set_target_temp(temp_c).await?;

        debug!(
            "Accessory '{}': Set heating setpoint to {:.1}°C",
            self.name, temp_c
        );

        Ok(())
    }

    /// Get system mode
    ///
    /// Matter system modes:
    /// - 0x00: Off
    /// - 0x01: Auto
    /// - 0x03: Cool
    /// - 0x04: Heat
    pub async fn get_system_mode(&self) -> u8 {
        match self.thermostat.get_target_state().await {
            crate::devices::HeatingState::Off => 0x00,
            crate::devices::HeatingState::Heat => 0x04,
        }
    }

    /// Set system mode
    ///
    /// # Arguments
    ///
    /// * `mode` - System mode (0x00=off, 0x04=heat)
    pub async fn set_system_mode(&self, mode: u8) -> Result<()> {
        let state = match mode {
            0x00 => crate::devices::HeatingState::Off,
            0x04 => crate::devices::HeatingState::Heat,
            _ => {
                warn!(
                    "Accessory '{}': Unsupported system mode: {}",
                    self.name, mode
                );
                return Ok(()); // Ignore unsupported modes
            }
        };

        self.thermostat.set_target_state(state).await?;

        debug!("Accessory '{}': Set system mode to {:?}", self.name, state);

        Ok(())
    }

    /// Get running state (actual heating state)
    ///
    /// Bit 0: Heat state
    /// Bit 1: Cool state
    pub async fn get_running_state(&self) -> u16 {
        match self.thermostat.get_current_state().await {
            crate::devices::HeatingState::Off => 0x0000,
            crate::devices::HeatingState::Heat => 0x0001,
        }
    }

    /// Get endpoint ID
    pub fn endpoint_id(&self) -> u16 {
        self.endpoint_id
    }

    /// Get accessory name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Matter server
///
/// Manages Matter protocol communication and thermostat accessories
pub struct MatterServer {
    /// Server port
    port: u16,

    /// Registered thermostat accessories
    accessories: HashMap<String, ThermostatAccessory>,

    /// Next available endpoint ID
    next_endpoint_id: u16,

    /// Matter device handle (FFI)
    matter_device: Option<Arc<RwLock<ffi::Device>>>,
}

impl MatterServer {
    /// Create a new Matter server
    ///
    /// # Arguments
    ///
    /// * `port` - Port to listen on (default: 5540)
    pub async fn new(port: u16) -> Result<Self> {
        info!("Initializing Matter server on port {}", port);

        Ok(Self {
            port,
            accessories: HashMap::new(),
            next_endpoint_id: 1, // Endpoint 0 is reserved for root node
            matter_device: None,
        })
    }

    /// Add a thermostat to the Matter server
    ///
    /// # Arguments
    ///
    /// * `name` - Zone name
    /// * `thermostat` - Thermostat device
    pub async fn add_thermostat(&mut self, name: &str, thermostat: Arc<Thermostat>) -> Result<()> {
        let endpoint_id = self.next_endpoint_id;
        self.next_endpoint_id += 1;

        let accessory = ThermostatAccessory::new(name.to_string(), thermostat, endpoint_id);

        self.accessories.insert(name.to_string(), accessory);

        info!(
            "Added thermostat '{}' to Matter server on endpoint {}",
            name, endpoint_id
        );

        Ok(())
    }

    /// Get accessory by name
    pub fn get_accessory(&self, name: &str) -> Option<&ThermostatAccessory> {
        self.accessories.get(name)
    }

    /// Initialize Matter stack
    ///
    /// This initializes the Connected Home IP Matter stack with:
    /// - Device attestation credentials
    /// - Commissioning information
    /// - Network configuration
    /// - Cluster handlers for each thermostat
    async fn initialize_matter_stack(&mut self) -> Result<()> {
        info!("Initializing Matter stack (Connected Home IP)");

        // Create Matter device configuration
        let device_config = ffi::DeviceConfig {
            vendor_id: VENDOR_ID,
            product_id: PRODUCT_ID,
            device_name: "Thermos Multi-Zone".to_string(),
            serial_number: format!("THERMO-{}", chrono::Utc::now().timestamp()),
            discriminator: 3840, // Default discriminator
            setup_passcode: 20202021, // Default setup passcode
            port: self.port,
        };

        // Initialize Matter device
        let device = ffi::Device::new(device_config)?;

        // Display commissioning information
        let qr_code = device.get_qr_code()?;
        let pairing_code = device.get_pairing_code()?;

        info!("═══════════════════════════════════════════════");
        info!("Matter Device Commissioning Information");
        info!("═══════════════════════════════════════════════");
        info!("QR Code: {}", qr_code);
        info!("Manual Pairing Code: {}", pairing_code);
        info!("═══════════════════════════════════════════════");
        info!("Scan the QR code with your Matter controller");
        info!("(Apple Home, Google Home, Amazon Alexa, etc.)");
        info!("═══════════════════════════════════════════════");

        let device = Arc::new(RwLock::new(device));

        // Create Matter thermostats for each accessory
        for (name, accessory) in self.accessories.iter_mut() {
            let thermostat_config = ffi::ThermostatConfig::new(
                accessory.endpoint_id(),
                name.clone(),
            );

            let device_guard = device.read().await;
            let matter_thermostat = ffi::Thermostat::new(&device_guard, thermostat_config)?;
            drop(device_guard);

            let matter_thermostat = Arc::new(RwLock::new(matter_thermostat));
            accessory.set_matter_thermostat(matter_thermostat.clone());

            info!("Registered Matter thermostat for zone '{}'", name);
        }

        self.matter_device = Some(device);

        info!(
            "Matter device initialized with {} thermostats",
            self.accessories.len()
        );

        Ok(())
    }

    /// Run the Matter server
    ///
    /// This starts the Matter protocol stack and begins advertising the device
    /// for commissioning.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting Matter server on port {}", self.port);

        // Initialize Matter stack
        self.initialize_matter_stack().await?;

        info!("Matter server is running");
        info!(
            "Commission this device using your Matter controller (Apple Home, Google Home, etc.)"
        );
        info!("Scan the QR code or enter pairing code when prompted");

        // Clone device handle for event loop
        let matter_device = self.matter_device.clone();

        // Spawn background task to run Matter event loop
        tokio::spawn(async move {
            if let Some(device) = matter_device {
                loop {
                    // Run Matter event loop (non-blocking)
                    {
                        let mut device_guard = device.write().await;
                        if let Err(e) = device_guard.run_event_loop() {
                            warn!("Matter event loop error: {}", e);
                        }
                    }

                    // Sleep for a short time to avoid busy-waiting
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        });

        // Spawn background task to sync thermostat states
        self.spawn_state_sync_task().await;

        // Keep server running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            debug!("Matter server heartbeat");
        }
    }

    /// Spawn background task to synchronize thermostat states with Matter
    async fn spawn_state_sync_task(&self) {
        for (name, accessory) in &self.accessories {
            let thermostat = accessory.thermostat.clone();
            let matter_thermostat = accessory.matter_thermostat.clone();
            let zone_name = name.clone();

            tokio::spawn(async move {
                loop {
                    // Update Matter with current thermostat state
                    if let Some(ref mt) = matter_thermostat {
                        let current_temp = thermostat.get_current_temp().await;
                        let current_state = thermostat.get_current_state().await;

                        let mut mt_guard = mt.write().await;

                        // Update temperature
                        if let Err(e) = mt_guard.set_local_temperature(current_temp) {
                            debug!("Failed to update temperature for '{}': {}", zone_name, e);
                        }

                        // Update running state
                        let running_state = match current_state {
                            crate::devices::HeatingState::Off => ffi::ThermostatRunningState::Idle,
                            crate::devices::HeatingState::Heat => ffi::ThermostatRunningState::Heat,
                        };

                        if let Err(e) = mt_guard.set_running_state(running_state) {
                            debug!("Failed to update running state for '{}': {}", zone_name, e);
                        }
                    }

                    // Sync every 5 seconds
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            });
        }
    }

    /// Generate commissioning QR code
    ///
    /// Returns a QR code string that can be scanned by Matter controllers
    pub fn generate_qr_code(&self) -> String {
        // Matter QR code format: MT:Y.K9042C00KA0648G00
        // This is a placeholder - real implementation would generate proper QR code
        // based on discriminator, passcode, and device attestation

        #[allow(clippy::useless_format)]
        let qr_code = format!("MT:Y.K9042C00KA0648G00");

        info!("Matter commissioning QR code: {}", qr_code);

        qr_code
    }

    /// Generate manual pairing code
    ///
    /// Returns an 11-digit manual pairing code
    pub fn generate_pairing_code(&self) -> String {
        // Matter manual pairing code format: 34970112332
        // This is a placeholder - real implementation would generate proper code
        // based on passcode and discriminator

        let pairing_code = "34970112332".to_string();

        info!("Matter manual pairing code: {}", pairing_code);

        pairing_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matter_constants() {
        assert_eq!(MATTER_PORT, 5540);
        assert_eq!(DEVICE_TYPE_THERMOSTAT, 0x0301);
        assert_eq!(CLUSTER_THERMOSTAT, 0x0201);
    }

    #[test]
    fn test_temperature_conversion() {
        // 20.5°C should be 2050 in centi-degrees
        let temp_c = 20.5;
        let centi_degrees = (temp_c * 100.0) as i16;
        assert_eq!(centi_degrees, 2050);

        // Convert back
        let temp_c_back = centi_degrees as f32 / 100.0;
        assert_eq!(temp_c_back, 20.5);
    }

    #[tokio::test]
    async fn test_matter_server_creation() {
        let server = MatterServer::new(5540).await.unwrap();
        assert_eq!(server.port, 5540);
        assert_eq!(server.accessories.len(), 0);
        assert_eq!(server.next_endpoint_id, 1);
    }

    #[test]
    fn test_qr_code_generation() {
        let server = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { MatterServer::new(5540).await.unwrap() });

        let qr_code = server.generate_qr_code();
        assert!(qr_code.starts_with("MT:"));
    }

    #[test]
    fn test_pairing_code_generation() {
        let server = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { MatterServer::new(5540).await.unwrap() });

        let pairing_code = server.generate_pairing_code();
        assert_eq!(pairing_code.len(), 11);
    }
}
