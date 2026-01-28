//! Matter Thermostat Cluster handler implementation
//!
//! Implements the thermostat cluster using the ThermostatHooks trait
//! for device-specific logic. Based on rs-matter's OnOffHandler pattern.
//!
//! # Architecture
//!
//! The handler wraps a `ThermostatHooks` implementation and provides:
//! - Attribute read/write handling
//! - Background task for out-of-band notifications
//! - Data version tracking for subscriptions
//!
//! # Usage
//!
//! ```ignore
//! let handler = ThermostatHandler::new(dataver, endpoint_id, zone_state);
//! handler.init();
//! // Run in background task
//! handler.run(ctx).await;
//! ```

use core::pin::pin;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;

use rs_matter::dm::{Cluster, Dataver, HandlerContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;

use crate::matter::cluster::{
    AttributeId, ControlSequenceOfOperationEnum, OutOfBandMessage, RelayStateBitmap,
    SystemModeEnum, ThermostatHooks,
};

/// Handler for Matter Thermostat Cluster (0x0201)
///
/// This handler manages the thermostat state machine and bridges
/// Matter commands to the device-specific ThermostatHooks implementation.
///
/// # Type Parameters
///
/// * `H` - The hooks implementation providing device-specific behavior
pub struct ThermostatHandler<H: ThermostatHooks> {
    dataver: Dataver,
    endpoint_id: u16,
    hooks: H,
    state_change_signal: Signal<NoopRawMutex, OutOfBandMessage>,
}

impl<H: ThermostatHooks> ThermostatHandler<H> {
    /// Create a new ThermostatHandler
    ///
    /// # Arguments
    ///
    /// * `dataver` - Data version tracker for subscription updates
    /// * `endpoint_id` - Matter endpoint ID for this thermostat
    /// * `hooks` - Device-specific hooks implementation
    pub fn new(dataver: Dataver, endpoint_id: u16, hooks: H) -> Self {
        Self {
            dataver,
            endpoint_id,
            hooks,
            state_change_signal: Signal::new(),
        }
    }

    /// Validate cluster configuration on init
    ///
    /// Panics if required attributes are missing from the cluster configuration.
    fn validate(&self) {
        // Ensure required attributes are present
        if H::CLUSTER
            .attribute(AttributeId::LocalTemperature as _)
            .is_none()
        {
            panic!("Thermostat validation: missing required attribute: LocalTemperature");
        }
        if H::CLUSTER.attribute(AttributeId::SystemMode as _).is_none() {
            panic!("Thermostat validation: missing required attribute: SystemMode");
        }
        if H::CLUSTER
            .attribute(AttributeId::OccupiedHeatingSetpoint as _)
            .is_none()
        {
            panic!("Thermostat validation: missing required attribute: OccupiedHeatingSetpoint");
        }
    }

    /// Initialize the handler
    ///
    /// Should be called once before using the handler.
    /// Validates the cluster configuration.
    pub fn init(&self) {
        self.validate();
    }

    /// Notify that state has changed (for out-of-band updates)
    ///
    /// Call this when state changes externally (not via Matter commands)
    /// to push updates to connected controllers.
    pub fn notify_state_changed(&self, message: OutOfBandMessage) {
        self.state_change_signal.signal(message);
    }

    /// Get the endpoint ID
    pub fn endpoint_id(&self) -> u16 {
        self.endpoint_id
    }

    // --- Attribute Accessors ---

    /// Get current local temperature
    pub fn local_temperature(&self) -> Nullable<i16> {
        self.hooks.local_temperature()
    }

    /// Get current setpoint
    pub fn occupied_heating_setpoint(&self) -> i16 {
        self.hooks.occupied_heating_setpoint()
    }

    /// Set setpoint (called from Matter write)
    pub fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error> {
        self.hooks.set_occupied_heating_setpoint(value)
    }

    /// Get minimum heat setpoint limit
    pub fn min_heat_setpoint_limit(&self) -> i16 {
        self.hooks.min_heat_setpoint_limit()
    }

    /// Get maximum heat setpoint limit
    pub fn max_heat_setpoint_limit(&self) -> i16 {
        self.hooks.max_heat_setpoint_limit()
    }

    /// Get current system mode
    pub fn system_mode(&self) -> SystemModeEnum {
        self.hooks.system_mode()
    }

    /// Set system mode (called from Matter write)
    ///
    /// For heating-only, only allows Off or Heat modes.
    pub fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error> {
        // For heating-only, only allow Off or Heat
        match mode {
            SystemModeEnum::Off | SystemModeEnum::Heat => self.hooks.set_system_mode(mode),
            _ => Err(ErrorCode::ConstraintError.into()),
        }
    }

    /// Get thermostat running state
    pub fn thermostat_running_state(&self) -> RelayStateBitmap {
        self.hooks.thermostat_running_state()
    }

    /// Get control sequence of operation
    pub fn control_sequence_of_operation(&self) -> ControlSequenceOfOperationEnum {
        self.hooks.control_sequence_of_operation()
    }
}

// --- ClusterAsyncHandler-like Implementation ---
//
// Note: The exact trait implementation depends on rs-matter version.
// This provides the core methods that would be used by ClusterAsyncHandler.

impl<H: ThermostatHooks> ThermostatHandler<H> {
    /// The cluster metadata for this handler
    pub const CLUSTER: Cluster<'static> = H::CLUSTER;

    /// Get current data version
    pub fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    /// Mark data version as changed
    ///
    /// Call this when any attribute value changes to trigger
    /// subscription updates.
    pub fn dataver_changed(&self) {
        self.dataver.changed();
    }

    /// Run the handler's background task
    ///
    /// This task:
    /// 1. Runs the hooks' background task
    /// 2. Listens for out-of-band state changes
    /// 3. Notifies Matter of attribute changes
    ///
    /// # Returns
    ///
    /// Never returns under normal operation. Returns error if hooks task fails.
    pub async fn run(&self, ctx: impl HandlerContext) -> Result<(), Error> {
        let mut hooks_fut = pin!(self.hooks.run(|msg| self.notify_state_changed(msg)));

        loop {
            match select(&mut hooks_fut, self.state_change_signal.wait()).await {
                Either::First(_) => {
                    // Hooks future should never complete
                    panic!("ThermostatHooks::run returned unexpectedly");
                }
                Either::Second(message) => {
                    // Handle out-of-band state change
                    self.handle_out_of_band_message(message, &ctx);
                }
            }
        }
    }

    /// Handle an out-of-band state change message
    fn handle_out_of_band_message(&self, message: OutOfBandMessage, ctx: &impl HandlerContext) {
        match message {
            OutOfBandMessage::TemperatureUpdate => {
                self.dataver_changed();
                ctx.notify_attribute_changed(
                    self.endpoint_id,
                    Self::CLUSTER.id,
                    AttributeId::LocalTemperature as _,
                );
            }
            OutOfBandMessage::SetpointChanged => {
                self.dataver_changed();
                ctx.notify_attribute_changed(
                    self.endpoint_id,
                    Self::CLUSTER.id,
                    AttributeId::OccupiedHeatingSetpoint as _,
                );
            }
            OutOfBandMessage::HeatingStateChanged => {
                self.dataver_changed();
                ctx.notify_attribute_changed(
                    self.endpoint_id,
                    Self::CLUSTER.id,
                    AttributeId::ThermostatRunningState as _,
                );
            }
            OutOfBandMessage::ModeChanged => {
                self.dataver_changed();
                ctx.notify_attribute_changed(
                    self.endpoint_id,
                    Self::CLUSTER.id,
                    AttributeId::SystemMode as _,
                );
            }
        }
    }
}
