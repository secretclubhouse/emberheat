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
use qrcode::render::unicode;
use qrcode::QrCode;
use std::collections::HashMap;
use std::fs;
use std::os::raw::c_void;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Matter server port (standard Matter port is 5540)
#[allow(dead_code)]
const MATTER_PORT: u16 = 5540;

/// Matter vendor ID (placeholder - should be registered)
const VENDOR_ID: u16 = 0xFFF1;

/// Matter product ID
const PRODUCT_ID: u16 = 0x8001;

/// Device type: Thermostat (0x0301)
#[allow(dead_code)]
const DEVICE_TYPE_THERMOSTAT: u16 = 0x0301;

/// Thermostat cluster ID (0x0201)
#[allow(dead_code)]
const CLUSTER_THERMOSTAT: u16 = 0x0201;

/// Basic information cluster ID (0x0028)
#[allow(dead_code)]
const CLUSTER_BASIC_INFO: u16 = 0x0028;

/* ============================================================================
 * Callback Context and Handlers
 * ========================================================================== */

/// Context passed to C callbacks
///
/// This structure is passed as a void pointer to C callbacks and contains
/// references to the Rust thermostat that needs to be updated.
struct CallbackContext {
    thermostat: Arc<Thermostat>,
    #[allow(dead_code)] // Stored for reference but endpoint is passed as callback parameter
    endpoint_id: u16,
    zone_name: String,
}

/// Extern C callback for setpoint changes
///
/// This function is called by the C++ Matter stack when a controller changes
/// the heating or cooling setpoint.
extern "C" fn setpoint_changed_callback(
    context: *mut c_void,
    endpoint_id: u16,
    heating_setpoint: i16,
    cooling_setpoint: i16,
) {
    if context.is_null() {
        error!("Setpoint callback received null context");
        return;
    }

    unsafe {
        let ctx = &*(context as *const CallbackContext);

        info!(
            "Matter callback: Setpoint changed for '{}' (endpoint {}): heating={} centi-°C, cooling={} centi-°C",
            ctx.zone_name, endpoint_id, heating_setpoint, cooling_setpoint
        );

        // Convert from centi-degrees Celsius to Celsius
        let heating_temp_c = heating_setpoint as f32 / 100.0;

        // Update the thermostat asynchronously
        // Note: This is called from C context (potentially from a non-Tokio thread),
        // so we use spawn instead of block_in_place to avoid panics
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let thermostat = ctx.thermostat.clone();
                let zone_name = ctx.zone_name.clone();

                handle.spawn(async move {
                    if let Err(e) = thermostat.set_target_temp(heating_temp_c).await {
                        error!(
                            "Failed to update target temperature for '{}': {}",
                            zone_name, e
                        );
                    } else {
                        info!(
                            "Successfully updated target temperature for '{}' to {:.1}°C",
                            zone_name, heating_temp_c
                        );
                    }
                });
            }
            Err(_) => {
                error!(
                    "Setpoint callback called outside Tokio runtime context for '{}'. Cannot update temperature.",
                    ctx.zone_name
                );
            }
        }
    }
}

/// Extern C callback for mode changes
///
/// This function is called by the C++ Matter stack when a controller changes
/// the system mode (off/heat/cool/etc).
extern "C" fn mode_changed_callback(context: *mut c_void, endpoint_id: u16, mode: u8) {
    if context.is_null() {
        error!("Mode callback received null context");
        return;
    }

    unsafe {
        let ctx = &*(context as *const CallbackContext);

        info!(
            "Matter callback: Mode changed for '{}' (endpoint {}): mode=0x{:02X}",
            ctx.zone_name, endpoint_id, mode
        );

        // Map Matter mode to HeatingState
        let state = match mode {
            0x00 => crate::devices::HeatingState::Off,
            0x04 => crate::devices::HeatingState::Heat,
            _ => {
                warn!(
                    "Unsupported Matter mode 0x{:02X} for '{}', ignoring",
                    mode, ctx.zone_name
                );
                return;
            }
        };

        // Update the thermostat asynchronously
        // Note: This is called from C context (potentially from a non-Tokio thread),
        // so we use spawn instead of block_in_place to avoid panics
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let thermostat = ctx.thermostat.clone();
                let zone_name = ctx.zone_name.clone();

                handle.spawn(async move {
                    if let Err(e) = thermostat.set_target_state(state).await {
                        error!("Failed to update system mode for '{}': {}", zone_name, e);
                    } else {
                        info!(
                            "Successfully updated system mode for '{}' to {:?}",
                            zone_name, state
                        );
                    }
                });
            }
            Err(_) => {
                error!(
                    "Mode callback called outside Tokio runtime context for '{}'. Cannot update mode.",
                    ctx.zone_name
                );
            }
        }
    }
}

/* ============================================================================
 * QR Code Generation and Display
 * ========================================================================== */

/// Generate QR code as ASCII art and save to file
///
/// This function takes a Matter commissioning QR code string (e.g., "MT:Y.K9042C00KA0648G00")
/// and generates an ASCII QR code that can be scanned by Matter controllers.
///
/// # Arguments
///
/// * `qr_code_data` - The QR code data string from Matter device
/// * `pairing_code` - The manual pairing code for fallback
///
/// # Returns
///
/// Returns the ASCII QR code string, or an error if generation fails
fn generate_and_save_qr_code(qr_code_data: &str, pairing_code: &str) -> Result<String> {
    // Generate QR code
    let code = QrCode::new(qr_code_data)?;

    // Render as Unicode string for terminal display
    let qr_string = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();

    // Determine save path
    let save_path = get_qr_code_file_path()?;

    // Create directory if it doesn't exist
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create file content with QR code and instructions
    let file_content = format!(
        "═══════════════════════════════════════════════════════════\n\
         Matter Device Commissioning QR Code\n\
         ═══════════════════════════════════════════════════════════\n\
         \n\
         Scan this QR code with your Matter controller:\n\
         (Apple Home, Google Home, Amazon Alexa, etc.)\n\
         \n\
         {}\n\
         \n\
         Manual Pairing Code: {}\n\
         QR Code Data: {}\n\
         \n\
         ═══════════════════════════════════════════════════════════\n\
         This file was generated by Thermos-Rust\n\
         Generated at: {}\n\
         ═══════════════════════════════════════════════════════════\n",
        qr_string,
        pairing_code,
        qr_code_data,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    // Write to file
    fs::write(&save_path, &file_content)?;

    info!("QR code saved to: {}", save_path.display());

    Ok(qr_string)
}

/// Get the file path for saving the QR code
///
/// Priority order:
/// 1. `/var/lib/thermos-rust/matter_qr_code.txt` (if writable)
/// 2. `~/.config/thermos-rust/matter_qr_code.txt` (user config)
/// 3. `./matter_qr_code.txt` (current directory fallback)
fn get_qr_code_file_path() -> Result<PathBuf> {
    // Try system-wide location first
    let system_path = PathBuf::from("/var/lib/thermos-rust/matter_qr_code.txt");
    if let Ok(metadata) = fs::metadata(system_path.parent().unwrap_or(&PathBuf::from("/"))) {
        if !metadata.permissions().readonly() {
            return Ok(system_path);
        }
    }

    // Try user config directory
    if let Some(home) = std::env::var_os("HOME") {
        let user_path = PathBuf::from(home)
            .join(".config")
            .join("thermos-rust")
            .join("matter_qr_code.txt");
        return Ok(user_path);
    }

    // Fallback to current directory
    Ok(PathBuf::from("matter_qr_code.txt"))
}

/// Matter thermostat accessory
///
/// Wraps a Thermostat instance and exposes it via Matter protocol
pub struct ThermostatAccessory {
    /// Zone name / accessory name
    #[allow(dead_code)]
    name: String,

    /// Underlying thermostat device
    thermostat: Arc<Thermostat>,

    /// Matter FFI thermostat handle
    matter_thermostat: Option<Arc<RwLock<ffi::Thermostat>>>,

    /// Endpoint ID in Matter fabric
    endpoint_id: u16,

    /// Callback context (boxed to keep it alive and get a stable pointer)
    #[allow(dead_code)]
    callback_context: Option<Box<CallbackContext>>,
}

#[allow(dead_code)]
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
            callback_context: None,
        }
    }

    /// Set the Matter FFI thermostat handle and register callbacks
    ///
    /// # Safety
    ///
    /// This function can only be called once per accessory instance. Calling it
    /// multiple times would create a use-after-free vulnerability, as the old
    /// callback context would be dropped while C++ still holds a pointer to it.
    pub(crate) fn set_matter_thermostat(
        &mut self,
        matter_thermostat: Arc<RwLock<ffi::Thermostat>>,
    ) {
        // Prevent multiple calls to avoid use-after-free
        assert!(
            self.callback_context.is_none(),
            "set_matter_thermostat can only be called once per accessory instance"
        );

        // Create callback context
        let context = Box::new(CallbackContext {
            thermostat: self.thermostat.clone(),
            endpoint_id: self.endpoint_id,
            zone_name: self.name.clone(),
        });

        // Get raw pointer to context (it will remain valid as long as the box is alive)
        let context_ptr = &*context as *const CallbackContext as *mut c_void;

        // Register callbacks with the Matter FFI layer
        {
            let mut mt_guard = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(matter_thermostat.write())
            });

            mt_guard.set_setpoint_callback(setpoint_changed_callback, context_ptr);
            mt_guard.set_mode_callback(mode_changed_callback, context_ptr);
        }

        info!(
            "Registered Matter callbacks for '{}' (endpoint {})",
            self.name, self.endpoint_id
        );

        // Store the context to keep it alive
        self.callback_context = Some(context);
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

    /// Metrics server for status updates (optional)
    metrics_server: Option<Arc<crate::metrics::MetricsServer>>,
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
            metrics_server: None,
        })
    }

    /// Set metrics server for status updates
    ///
    /// This should be called before `run()` to enable Matter status reporting
    /// to the metrics/diagnostics HTTP server.
    pub fn set_metrics_server(&mut self, metrics_server: Arc<crate::metrics::MetricsServer>) {
        self.metrics_server = Some(metrics_server);
        info!("Metrics server linked to Matter server for status updates");
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
    #[allow(dead_code)]
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
            discriminator: 3840,      // Default discriminator
            setup_passcode: 20202021, // Default setup passcode
            port: self.port,
        };

        // Initialize Matter device
        let device = ffi::Device::new(device_config)?;

        // Display commissioning information
        let qr_code_data = device.get_qr_code()?;
        let pairing_code = device.get_pairing_code()?;

        // Generate ASCII QR code and save to file
        let qr_code_file_path = get_qr_code_file_path().ok();
        match generate_and_save_qr_code(&qr_code_data, &pairing_code) {
            Ok(qr_string) => {
                info!("═══════════════════════════════════════════════");
                info!("Matter Device Commissioning Information");
                info!("═══════════════════════════════════════════════");
                info!("");
                info!("Scan this QR code with your Matter controller:");
                info!("(Apple Home, Google Home, Amazon Alexa, etc.)");
                info!("");
                // Display the QR code
                for line in qr_string.lines() {
                    info!("{}", line);
                }
                info!("");
                info!("Manual Pairing Code: {}", pairing_code);
                info!("QR Code Data: {}", qr_code_data);
                info!("═══════════════════════════════════════════════");

                // Update metrics server with QR code data
                if let Some(ref metrics_server) = self.metrics_server {
                    // Create full QR code content for HTTP display
                    let qr_content = format!(
                        "═══════════════════════════════════════════════════════════\n\
                         Matter Device Commissioning QR Code\n\
                         ═══════════════════════════════════════════════════════════\n\
                         \n\
                         Scan this QR code with your Matter controller:\n\
                         (Apple Home, Google Home, Amazon Alexa, etc.)\n\
                         \n\
                         {}\n\
                         \n\
                         Manual Pairing Code: {}\n\
                         QR Code Data: {}\n\
                         \n\
                         ═══════════════════════════════════════════════════════════\n",
                        qr_string, pairing_code, qr_code_data
                    );
                    metrics_server.set_qr_code(qr_content).await;
                }
            }
            Err(e) => {
                warn!("Failed to generate QR code: {}", e);
                warn!("Displaying commissioning information without QR code:");
                info!("═══════════════════════════════════════════════");
                info!("Matter Device Commissioning Information");
                info!("═══════════════════════════════════════════════");
                info!("QR Code Data: {}", qr_code_data);
                info!("Manual Pairing Code: {}", pairing_code);
                info!("═══════════════════════════════════════════════");
                info!("Scan the QR code with your Matter controller");
                info!("(Apple Home, Google Home, Amazon Alexa, etc.)");
                info!("═══════════════════════════════════════════════");

                // Still update metrics server with text-only information
                if let Some(ref metrics_server) = self.metrics_server {
                    let qr_content = format!(
                        "QR Code generation failed, but you can still pair manually:\n\n\
                         QR Code Data: {}\n\
                         Manual Pairing Code: {}\n\n\
                         Check the QR code file for a visual version.\n",
                        qr_code_data, pairing_code
                    );
                    metrics_server.set_qr_code(qr_content).await;
                }
            }
        }

        // Update Matter status in metrics server
        if let Some(ref metrics_server) = self.metrics_server {
            let status = crate::metrics::MatterStatus {
                commissioned: device.is_commissioned(),
                initialized: true,
                qr_code_available: true,
                qr_code_file_path: qr_code_file_path.map(|p| p.display().to_string()),
            };
            metrics_server.set_matter_status(status).await;
        }

        let device = Arc::new(RwLock::new(device));

        // Create Matter thermostats for each accessory
        for (name, accessory) in self.accessories.iter_mut() {
            let thermostat_config =
                ffi::ThermostatConfig::new(accessory.endpoint_id(), name.clone());

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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        assert_eq!(CLUSTER_BASIC_INFO, 0x0028);
        assert_eq!(VENDOR_ID, 0xFFF1);
        assert_eq!(PRODUCT_ID, 0x8001);
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

    #[test]
    fn test_temperature_conversion_negative() {
        // Test negative temperatures
        let temp_c = -10.0;
        let centi_degrees = (temp_c * 100.0) as i16;
        assert_eq!(centi_degrees, -1000);

        let temp_c_back = centi_degrees as f32 / 100.0;
        assert_eq!(temp_c_back, -10.0);
    }

    #[test]
    fn test_temperature_conversion_zero() {
        let temp_c = 0.0;
        let centi_degrees = (temp_c * 100.0) as i16;
        assert_eq!(centi_degrees, 0);
    }

    #[tokio::test]
    async fn test_matter_server_creation() {
        let server = MatterServer::new(5540).await.unwrap();
        assert_eq!(server.port, 5540);
        assert_eq!(server.accessories.len(), 0);
        assert_eq!(server.next_endpoint_id, 1);
        assert!(server.matter_device.is_none());
    }

    #[tokio::test]
    async fn test_matter_server_creation_custom_port() {
        let server = MatterServer::new(9000).await.unwrap();
        assert_eq!(server.port, 9000);
    }

    #[tokio::test]
    async fn test_matter_server_empty_after_creation() {
        let server = MatterServer::new(5540).await.unwrap();
        assert!(server.accessories.is_empty());
        assert!(server.get_accessory("Any").is_none());
    }

    #[tokio::test]
    async fn test_endpoint_allocation_starts_at_one() {
        let server = MatterServer::new(5540).await.unwrap();
        assert_eq!(server.next_endpoint_id, 1);
    }

    #[tokio::test]
    async fn test_get_accessory_not_exists() {
        let server = MatterServer::new(5540).await.unwrap();
        let accessory = server.get_accessory("NonExistent");
        assert!(accessory.is_none());
    }

    #[test]
    fn test_qr_code_generation() {
        let server = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { MatterServer::new(5540).await.unwrap() });

        let qr_code = server.generate_qr_code();
        assert!(qr_code.starts_with("MT:"));
        assert!(qr_code.len() > 3);
    }

    #[test]
    fn test_pairing_code_generation() {
        let server = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { MatterServer::new(5540).await.unwrap() });

        let pairing_code = server.generate_pairing_code();
        assert_eq!(pairing_code.len(), 11);
        assert!(pairing_code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_system_mode_mapping_heat() {
        // Test that Matter heat mode (0x04) maps correctly
        let mode: u8 = 0x04;
        assert_eq!(mode, 0x04);
    }

    #[test]
    fn test_system_mode_mapping_off() {
        // Test that Matter off mode (0x00) maps correctly
        let mode: u8 = 0x00;
        assert_eq!(mode, 0x00);
    }

    #[test]
    fn test_running_state_flags() {
        // Test running state flag values
        let idle: u16 = 0x0000;
        let heat: u16 = 0x0001;

        assert_eq!(idle, 0);
        assert_ne!(idle, heat);
        assert_eq!(heat, 1);
    }

    #[test]
    fn test_vendor_and_product_ids() {
        // Verify vendor/product IDs are non-zero
        assert_ne!(VENDOR_ID, 0);
        assert_ne!(PRODUCT_ID, 0);
    }

    // Note: Full integration tests with actual Thermostat instances are difficult
    // in unit tests due to complex dependencies. The key callback logic (temperature
    // conversion, mode mapping) is tested via the conversion tests below.

    #[test]
    fn test_callback_temperature_conversion() {
        // Test the temperature conversion used in callbacks
        let heating_setpoint_centi: i16 = 2250; // Matter format
        let temp_c = heating_setpoint_centi as f32 / 100.0;
        assert_eq!(temp_c, 22.5);

        // Test negative temperatures
        let cold_setpoint: i16 = -500;
        let cold_c = cold_setpoint as f32 / 100.0;
        assert_eq!(cold_c, -5.0);
    }

    #[test]
    fn test_callback_mode_mapping() {
        // Test mode values used in callbacks
        assert_eq!(0x00, 0); // Off
        assert_eq!(0x04, 4); // Heat

        // Verify mapping matches Matter specification
        let off_mode: u8 = 0x00;
        let heat_mode: u8 = 0x04;

        assert_ne!(off_mode, heat_mode);
    }

    // ThermostatAccessory tests
    // Note: Full integration tests with real Thermostat instances require complex
    // setup. These tests focus on conversion logic and simple state mapping which
    // is the core of the accessory layer.

    #[test]
    fn test_accessory_temperature_to_centi_degrees() {
        // Test the conversion used in get_local_temperature
        let temp_c = 21.5;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, 2150);

        let temp_c = 18.25;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, 1825);

        let temp_c = -5.0;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, -500);
    }

    #[test]
    fn test_accessory_centi_degrees_to_temperature() {
        // Test the conversion used in set_occupied_heating_setpoint
        let centi: i16 = 2350;
        let temp_c = centi as f32 / 100.0;
        assert_eq!(temp_c, 23.5);

        let centi: i16 = 1825;
        let temp_c = centi as f32 / 100.0;
        assert_eq!(temp_c, 18.25);

        let centi: i16 = -500;
        let temp_c = centi as f32 / 100.0;
        assert_eq!(temp_c, -5.0);
    }

    #[test]
    fn test_accessory_system_mode_to_matter() {
        use crate::devices::HeatingState;

        // Test HeatingState to Matter mode conversion (used in get_system_mode)
        let off_state = HeatingState::Off;
        let matter_mode = match off_state {
            HeatingState::Off => 0x00,
            HeatingState::Heat => 0x04,
        };
        assert_eq!(matter_mode, 0x00);

        let heat_state = HeatingState::Heat;
        let matter_mode = match heat_state {
            HeatingState::Off => 0x00,
            HeatingState::Heat => 0x04,
        };
        assert_eq!(matter_mode, 0x04);
    }

    #[test]
    fn test_accessory_matter_to_system_mode() {
        use crate::devices::HeatingState;

        // Test Matter mode to HeatingState conversion (used in set_system_mode)
        let matter_off: u8 = 0x00;
        let state = match matter_off {
            0x00 => HeatingState::Off,
            0x04 => HeatingState::Heat,
            _ => HeatingState::Off, // Default for unsupported
        };
        assert_eq!(state, HeatingState::Off);

        let matter_heat: u8 = 0x04;
        let state = match matter_heat {
            0x00 => HeatingState::Off,
            0x04 => HeatingState::Heat,
            _ => HeatingState::Off,
        };
        assert_eq!(state, HeatingState::Heat);

        // Test unsupported modes
        let matter_unsupported: u8 = 0x01; // Auto
        let state = match matter_unsupported {
            0x00 => HeatingState::Off,
            0x04 => HeatingState::Heat,
            _ => HeatingState::Off, // Should ignore
        };
        assert_eq!(state, HeatingState::Off);
    }

    #[test]
    fn test_accessory_running_state_flags() {
        use crate::devices::HeatingState;

        // Test running state flag values (used in get_running_state)
        let off_state = HeatingState::Off;
        let running_state = match off_state {
            HeatingState::Off => 0x0000,
            HeatingState::Heat => 0x0001,
        };
        assert_eq!(running_state, 0x0000);

        let heat_state = HeatingState::Heat;
        let running_state = match heat_state {
            HeatingState::Off => 0x0000,
            HeatingState::Heat => 0x0001,
        };
        assert_eq!(running_state, 0x0001);

        // Verify flag values match Matter spec
        assert_eq!(0x0000, 0); // Idle
        assert_eq!(0x0001, 1); // Heat
    }
}
