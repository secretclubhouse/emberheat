//! Matter stack integration for thermos-esp32
//!
//! Provides the glue between rs-matter-embassy and our zone thermostat states.
//!
//! # Design Decision
//!
//! This uses a thin wrapper approach (Option B from Phase 3 design) that delegates
//! directly to `ZoneThermostatState`. See `docs/plans/2026-01-27-phase3-matter-integration.md`
//! for rationale and future refactor path to move handler logic to thermos-matter.
//!
//! # Handler Pattern
//!
//! rs-matter-embassy uses handler chaining with `EmptyHandler.chain(...)`. Each zone
//! gets its own `ZoneThermostatCluster` handler chained for its endpoint.

use core::pin::pin;

use rs_matter_embassy::matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::devices::test::TEST_DEV_ATT;
use rs_matter_embassy::matter::dm::{
    Access, Async, Attribute, Cluster, Dataver, DeviceType, EmptyHandler, Endpoint, EpClMatcher,
    Handler, Node, NonBlockingHandler, Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter_embassy::matter::error::{Error, ErrorCode};
use rs_matter_embassy::matter::tlv::Nullable;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::matter::{clusters, devices, BasicCommData};
use rs_matter_embassy::rand::esp::esp_rand;
use rs_matter_embassy::stack::persist::DummyKvBlobStore;
use rs_matter_embassy::wireless::esp::EspWifiDriver;
#[cfg(feature = "external-ble")]
use rs_matter_embassy::wireless::{BleDriver, WifiCoexDriver, WifiDriver};
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiMatterStack};

use esp_hal::peripherals::{BT, WIFI};
use esp_radio::Controller as EspRadioController;

use log::{debug, info, warn};

use crate::config::{
    MATTER_DEVICE_NAME, MATTER_DISCRIMINATOR, MATTER_PASSCODE, MATTER_PRODUCT_ID, MATTER_VENDOR_ID,
    NUM_ZONES,
};
use crate::matter::zones::ZONE_STATES;

use thermos_matter::cluster::{ControlSequenceOfOperationEnum, RelayStateBitmap, SystemModeEnum};
use thermos_matter::state::ZoneThermostatState;
use thermos_matter::types::{defaults, SystemMode};

// =============================================================================
// Constants
// =============================================================================

/// Thermostat cluster ID (Matter spec 0x0201)
pub const CLUSTER_THERMOSTAT: u32 = 0x0201;

/// Matter device type for Thermostat (0x0301)
pub const DEV_TYPE_THERMOSTAT: DeviceType = DeviceType {
    dtype: 0x0301,
    drev: 2, // Device revision 2 per Matter 1.3 spec
};

/// First zone endpoint ID (endpoint 0 is root/system)
pub const FIRST_ZONE_ENDPOINT: u16 = 1;

/// Identify cluster ID (Matter spec 0x0003)
pub const CLUSTER_IDENTIFY: u32 = 0x0003;

/// Binding cluster ID (Matter spec 0x001E)
pub const CLUSTER_BINDING: u32 = 0x001E;

// =============================================================================
// Debug Helpers
// =============================================================================

/// Decode a Matter cluster ID to a human-readable name for logging
pub fn cluster_name(cluster_id: u32) -> &'static str {
    match cluster_id {
        0x001D => "Descriptor",
        0x001E => "Binding",
        0x001F => "AccessControl",
        0x0028 => "BasicInformation",
        0x0029 => "OtaProvider",
        0x002A => "OtaRequestor",
        0x002B => "LocalizationConfig",
        0x002C => "TimeFormatLocalization",
        0x002D => "UnitLocalization",
        0x002E => "PowerSourceConfig",
        0x002F => "PowerSource",
        0x0030 => "GeneralCommissioning",
        0x0031 => "NetworkCommissioning",
        0x0032 => "DiagnosticLogs",
        0x0033 => "GeneralDiagnostics",
        0x0034 => "SoftwareDiagnostics",
        0x0035 => "ThreadNetworkDiag",
        0x0036 => "WiFiNetworkDiag",
        0x0037 => "EthernetNetworkDiag",
        0x003C => "AdministratorCommissioning",
        0x003E => "OperationalCredentials",
        0x003F => "GroupKeyManagement",
        0x0040 => "FixedLabel",
        0x0041 => "UserLabel",
        0x0003 => "Identify",
        0x0004 => "Groups",
        0x0005 => "Scenes",
        0x0006 => "OnOff",
        0x0008 => "LevelControl",
        0x0201 => "Thermostat",
        0x0204 => "ThermostatUIConfig",
        0x0402 => "TemperatureMeasurement",
        _ if cluster_id > 0x10000000 => "VendorSpecific",
        _ => "Unknown",
    }
}

/// Log detailed info about a Matter attribute path
pub fn log_attr_access(op: &str, endpoint: u16, cluster: u32, attr: u32, list_index: Option<u16>) {
    let cluster_str = cluster_name(cluster);
    if let Some(idx) = list_index {
        info!(
            "Matter {}: ep={} cluster=0x{:04X}({}) attr=0x{:04X} list[{}]",
            op, endpoint, cluster, cluster_str, attr, idx
        );
    } else {
        info!(
            "Matter {}: ep={} cluster=0x{:04X}({}) attr=0x{:04X}",
            op, endpoint, cluster, cluster_str, attr
        );
    }
}

// =============================================================================
// Descriptor Cluster Logging Wrapper
// =============================================================================

/// Attribute IDs for Descriptor cluster (0x001D)
pub mod desc_attr {
    /// Device type list - what device types this endpoint implements
    pub const DEVICE_TYPE_LIST: u32 = 0x0000;
    /// Server list - cluster IDs this endpoint's server implements
    pub const SERVER_LIST: u32 = 0x0001;
    /// Client list - cluster IDs this endpoint's client implements
    pub const CLIENT_LIST: u32 = 0x0002;
    /// Parts list - child endpoint IDs (for aggregator/bridge)
    pub const PARTS_LIST: u32 = 0x0003;
}

/// Human-readable names for Descriptor attributes
fn desc_attr_name(attr_id: u32) -> &'static str {
    match attr_id {
        desc_attr::DEVICE_TYPE_LIST => "DeviceTypeList",
        desc_attr::SERVER_LIST => "ServerList",
        desc_attr::CLIENT_LIST => "ClientList",
        desc_attr::PARTS_LIST => "PartsList",
        0xFFFC => "FeatureMap",
        0xFFFD => "ClusterRevision",
        0xFFFE => "AttributeList",
        _ => "Unknown",
    }
}

/// Logging wrapper around the built-in DescHandler
///
/// This wraps rs-matter's Descriptor cluster handler to add logging
/// so we can trace what HomeKit is reading from the Descriptor cluster.
pub struct DescriptorLoggingHandler<H> {
    inner: H,
}

impl<H> DescriptorLoggingHandler<H> {
    pub fn new(inner: H, _endpoint_id: u16) -> Self {
        Self { inner }
    }
}

impl<H: Handler> Handler for DescriptorLoggingHandler<H> {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;
        let attr_name = desc_attr_name(attr_id);

        info!(
            "Matter READ: ep={} cluster=0x{:04X}(Descriptor) attr=0x{:04X}({})",
            attr.endpoint_id, attr.cluster_id, attr_id, attr_name
        );

        // For device-type-list, log extra info about what we expect to return
        if attr_id == desc_attr::DEVICE_TYPE_LIST {
            info!(
                "  -> Descriptor should return device_type=0x{:04X} (Thermostat) rev={}",
                DEV_TYPE_THERMOSTAT.dtype, DEV_TYPE_THERMOSTAT.drev
            );
        }

        let result = self.inner.read(ctx, reply);

        match &result {
            Ok(()) => {
                info!("  -> Descriptor read completed successfully");
            }
            Err(e) => {
                warn!("  -> Descriptor read FAILED: {:?}", e);
            }
        }

        result
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        info!(
            "Matter WRITE: ep={} cluster=0x{:04X}(Descriptor) attr=0x{:04X}",
            attr.endpoint_id, attr.cluster_id, attr.attr_id
        );
        self.inner.write(ctx)
    }

    fn invoke(
        &self,
        ctx: impl rs_matter_embassy::matter::dm::InvokeContext,
        reply: impl rs_matter_embassy::matter::dm::InvokeReply,
    ) -> Result<(), Error> {
        self.inner.invoke(ctx, reply)
    }
}

impl<H: NonBlockingHandler> NonBlockingHandler for DescriptorLoggingHandler<H> {}

// =============================================================================
// Identify Cluster (required by HomeKit for thermostat devices)
// =============================================================================

/// Identify cluster attribute IDs
pub mod identify_attr {
    /// Time remaining in identify mode (seconds)
    pub const IDENTIFY_TIME: u16 = 0x0000;
    /// Type of identification the device supports
    pub const IDENTIFY_TYPE: u16 = 0x0001;
}

/// Identify type enum values
#[repr(u8)]
pub enum IdentifyType {
    None = 0x00,
    LightOutput = 0x01,
    VisibleIndicator = 0x02,
    AudibleBeep = 0x03,
    Display = 0x04,
    Actuator = 0x05,
}

/// Simple Identify cluster handler
///
/// This is a minimal implementation that satisfies HomeKit's requirement
/// for an Identify cluster on thermostat endpoints.
pub struct IdentifyHandler {
    endpoint_id: u16,
    dataver: Dataver,
}

impl IdentifyHandler {
    pub fn new(endpoint_id: u16, dataver: Dataver) -> Self {
        Self {
            endpoint_id,
            dataver,
        }
    }
}

impl Handler for IdentifyHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        log_attr_access("READ", attr.endpoint_id, attr.cluster_id, attr_id, None);

        let Some(writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        match attr_id {
            x if x == identify_attr::IDENTIFY_TIME as u32 => {
                // Always return 0 (not currently identifying)
                Reply::set(writer, 0u16)?;
            }
            x if x == identify_attr::IDENTIFY_TYPE as u32 => {
                // We don't have any physical indicator
                Reply::set(writer, IdentifyType::None as u8)?;
            }
            _ => {
                return Err(ErrorCode::AttributeNotFound.into());
            }
        }

        Ok(())
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        match attr_id {
            x if x == identify_attr::IDENTIFY_TIME as u32 => {
                // Accept the write but don't do anything (no physical indicator)
                let _value: u16 = ctx.data().u16()?;
                info!("Identify: received identify request (no-op)");
                Ok(())
            }
            _ => Err(ErrorCode::UnsupportedAccess.into()),
        }
    }
}

impl NonBlockingHandler for IdentifyHandler {}

/// Identify cluster attributes
const IDENTIFY_ATTRIBUTES: &[Attribute] = &[
    // IdentifyTime (0x0000) - u16, read-write
    Attribute::new(
        identify_attr::IDENTIFY_TIME as u32,
        Access::RWVM,
        Quality::N,
    ),
    // IdentifyType (0x0001) - enum8, read-only
    Attribute::new(identify_attr::IDENTIFY_TYPE as u32, Access::RV, Quality::N),
];

/// Identify cluster metadata
pub const IDENTIFY_CLUSTER: Cluster<'static> = Cluster::new(
    CLUSTER_IDENTIFY, // cluster ID 0x0003
    4,                // revision
    0x00,             // feature map (no optional features)
    IDENTIFY_ATTRIBUTES,
    &[], // commands (Identify command not implemented)
    |_, _, _| true,
    |_, _, _| true,
);

// =============================================================================
// Thermostat User Interface Configuration Cluster (0x0204)
// =============================================================================

/// Thermostat UI Configuration cluster ID
pub const CLUSTER_THERMOSTAT_UI_CONFIG: u32 = 0x0204;

/// Thermostat UI Configuration attribute IDs
pub mod thermostat_ui_attr {
    /// Temperature display mode (Celsius=0, Fahrenheit=1)
    pub const TEMPERATURE_DISPLAY_MODE: u16 = 0x0000;
    /// Keypad lockout level (0-5)
    pub const KEYPAD_LOCKOUT: u16 = 0x0001;
}

/// Temperature display mode enum
#[repr(u8)]
pub enum TemperatureDisplayMode {
    Celsius = 0x00,
    Fahrenheit = 0x01,
}

/// Keypad lockout enum
#[repr(u8)]
#[allow(dead_code)]
pub enum KeypadLockout {
    NoLockout = 0x00,
    Lockout1 = 0x01,
    Lockout2 = 0x02,
    Lockout3 = 0x03,
    Lockout4 = 0x04,
    Lockout5 = 0x05,
}

/// Thermostat User Interface Configuration handler
///
/// Controls display settings like temperature unit (Celsius/Fahrenheit).
pub struct ThermostatUiConfigHandler {
    dataver: Dataver,
}

impl ThermostatUiConfigHandler {
    pub fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }
}

impl Handler for ThermostatUiConfigHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        log_attr_access("READ", attr.endpoint_id, attr.cluster_id, attr_id, None);

        let Some(writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        match attr_id {
            x if x == thermostat_ui_attr::TEMPERATURE_DISPLAY_MODE as u32 => {
                // Default to Celsius
                Reply::set(writer, TemperatureDisplayMode::Celsius as u8)?;
            }
            x if x == thermostat_ui_attr::KEYPAD_LOCKOUT as u32 => {
                // No lockout
                Reply::set(writer, KeypadLockout::NoLockout as u8)?;
            }
            _ => {
                return Err(ErrorCode::AttributeNotFound.into());
            }
        }

        Ok(())
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        match attr_id {
            x if x == thermostat_ui_attr::TEMPERATURE_DISPLAY_MODE as u32 => {
                // Accept the write (HomeKit may want to set display mode)
                let value: u8 = ctx.data().u8()?;
                if value > 1 {
                    return Err(ErrorCode::ConstraintError.into());
                }
                info!("ThermostatUIConfig: display mode set to {}", value);
                Ok(())
            }
            x if x == thermostat_ui_attr::KEYPAD_LOCKOUT as u32 => {
                // Accept the write
                let value: u8 = ctx.data().u8()?;
                if value > 5 {
                    return Err(ErrorCode::ConstraintError.into());
                }
                info!("ThermostatUIConfig: keypad lockout set to {}", value);
                Ok(())
            }
            _ => Err(ErrorCode::UnsupportedAccess.into()),
        }
    }
}

impl NonBlockingHandler for ThermostatUiConfigHandler {}

/// Thermostat UI Configuration cluster attributes
const THERMOSTAT_UI_CONFIG_ATTRIBUTES: &[Attribute] = &[
    // TemperatureDisplayMode (0x0000) - enum8, read-write
    Attribute::new(
        thermostat_ui_attr::TEMPERATURE_DISPLAY_MODE as u32,
        Access::RWVM,
        Quality::N,
    ),
    // KeypadLockout (0x0001) - enum8, read-write (requires manage access in spec)
    Attribute::new(
        thermostat_ui_attr::KEYPAD_LOCKOUT as u32,
        Access::RWVM,
        Quality::N,
    ),
];

/// Thermostat UI Configuration cluster metadata
pub const THERMOSTAT_UI_CONFIG_CLUSTER: Cluster<'static> = Cluster::new(
    CLUSTER_THERMOSTAT_UI_CONFIG, // cluster ID 0x0204
    2,                            // revision
    0x00,                         // feature map (no optional features)
    THERMOSTAT_UI_CONFIG_ATTRIBUTES,
    &[], // no commands
    |_, _, _| true,
    |_, _, _| true,
);

// =============================================================================
// Binding Cluster (0x001E)
// =============================================================================

/// Binding cluster attribute IDs
pub mod binding_attr {
    /// List of binding entries
    pub const BINDING: u16 = 0x0000;
}

/// Binding cluster handler
///
/// The Binding cluster allows a device to store bindings to other devices/groups.
/// For thermostats, this enables binding to external temperature sensors.
///
/// This is a minimal implementation that returns an empty binding list.
/// HomeKit/Eve uses this for external sensor integration.
pub struct BindingHandler {
    dataver: Dataver,
}

impl BindingHandler {
    pub fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }
}

impl Handler for BindingHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        log_attr_access("READ", attr.endpoint_id, attr.cluster_id, attr_id, None);

        let Some(writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(());
        };

        match attr_id {
            x if x == binding_attr::BINDING as u32 => {
                // Return empty binding list (no external sensors bound)
                // ToTLV is implemented for &[T], so empty slice works
                let empty: &[u8] = &[];
                Reply::set(writer, empty)?;
            }
            _ => {
                return Err(ErrorCode::AttributeNotFound.into());
            }
        }

        Ok(())
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        match attr_id {
            x if x == binding_attr::BINDING as u32 => {
                // Accept writes to binding list
                // In a full implementation, we'd parse and store the bindings
                // For now, just acknowledge the write
                info!("Binding: received binding write (not persisted)");
                Ok(())
            }
            _ => Err(ErrorCode::UnsupportedAccess.into()),
        }
    }
}

impl NonBlockingHandler for BindingHandler {}

/// Binding cluster attributes
const BINDING_ATTRIBUTES: &[Attribute] = &[
    // Binding (0x0000) - list of TargetStruct, read-write, fabric-scoped
    Attribute::new(binding_attr::BINDING as u32, Access::RWFA, Quality::N),
];

/// Binding cluster metadata
pub const BINDING_CLUSTER: Cluster<'static> = Cluster::new(
    CLUSTER_BINDING, // cluster ID 0x001E
    1,               // revision
    0x00,            // feature map (no optional features)
    BINDING_ATTRIBUTES,
    &[], // no commands
    |_, _, _| true,
    |_, _, _| true,
);

// =============================================================================
// Thermostat Attribute IDs (heating-only subset)
// =============================================================================

/// Thermostat attribute IDs from Matter specification
pub mod attr {
    /// Current temperature in 0.01°C units (nullable i16)
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    /// Absolute minimum heat setpoint limit (hardware limit) - mandatory with HEAT feature
    pub const ABS_MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0003;
    /// Absolute maximum heat setpoint limit (hardware limit) - mandatory with HEAT feature
    pub const ABS_MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0004;
    /// Absolute minimum cool setpoint limit (hardware limit) - mandatory with COOL feature
    pub const ABS_MIN_COOL_SETPOINT_LIMIT: u16 = 0x0005;
    /// Absolute maximum cool setpoint limit (hardware limit) - mandatory with COOL feature
    pub const ABS_MAX_COOL_SETPOINT_LIMIT: u16 = 0x0006;
    /// Target cooling temperature in 0.01°C units (i16)
    pub const OCCUPIED_COOLING_SETPOINT: u16 = 0x0011;
    /// Target heating temperature in 0.01°C units (i16)
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    /// Minimum allowed setpoint (user-adjustable limit, i16)
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    /// Maximum allowed setpoint (user-adjustable limit, i16)
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
    /// Minimum allowed cooling setpoint (user-adjustable limit, i16)
    pub const MIN_COOL_SETPOINT_LIMIT: u16 = 0x0017;
    /// Maximum allowed cooling setpoint (user-adjustable limit, i16)
    pub const MAX_COOL_SETPOINT_LIMIT: u16 = 0x0018;
    /// Control sequence: 0x02 = Heating Only
    pub const CONTROL_SEQUENCE_OF_OPERATION: u16 = 0x001B;
    /// System mode: 0x00 = Off, 0x04 = Heat
    pub const SYSTEM_MODE: u16 = 0x001C;
    /// Running state bitmap: bit 0 = Heat active
    pub const THERMOSTAT_RUNNING_STATE: u16 = 0x0029;
}

// =============================================================================
// Zone Thermostat Cluster Handler
// =============================================================================

/// Handler for a single zone's thermostat cluster
///
/// This is a thin wrapper that reads/writes state from `ZoneThermostatState`.
/// The actual hysteresis control logic runs in the control loop task.
///
/// Implements rs-matter's `Handler` trait to respond to Matter attribute
/// read/write operations.
pub struct ZoneThermostatCluster {
    /// Matter endpoint ID for this zone (1-based)
    endpoint_id: u16,
    /// Data version for change tracking
    dataver: Dataver,
}

impl ZoneThermostatCluster {
    /// Create a new thermostat cluster handler for a zone
    ///
    /// # Arguments
    ///
    /// * `endpoint_id` - Matter endpoint ID (1 = zone 0, 2 = zone 1, etc.)
    /// * `dataver` - Data version tracker for attribute changes
    pub fn new(endpoint_id: u16, dataver: Dataver) -> Self {
        Self {
            endpoint_id,
            dataver,
        }
    }

    /// Get the zone index (0-based) for this handler
    fn zone_index(&self) -> usize {
        (self.endpoint_id - FIRST_ZONE_ENDPOINT) as usize
    }

    /// Get reference to the zone state
    fn zone_state(&self) -> Option<&'static ZoneThermostatState> {
        ZONE_STATES.get(self.zone_index())
    }

    /// Get current data version
    pub fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    /// Mark data as changed (increments version)
    pub fn dataver_changed(&self) {
        self.dataver.changed();
    }
}

/// Human-readable names for Thermostat attributes
fn thermostat_attr_name(attr_id: u32) -> &'static str {
    match attr_id {
        0x0000 => "LocalTemperature",
        0x0003 => "AbsMinHeatSetpointLimit",
        0x0004 => "AbsMaxHeatSetpointLimit",
        0x0005 => "AbsMinCoolSetpointLimit",
        0x0006 => "AbsMaxCoolSetpointLimit",
        0x0011 => "OccupiedCoolingSetpoint",
        0x0012 => "OccupiedHeatingSetpoint",
        0x0015 => "MinHeatSetpointLimit",
        0x0016 => "MaxHeatSetpointLimit",
        0x0017 => "MinCoolSetpointLimit",
        0x0018 => "MaxCoolSetpointLimit",
        0x001B => "ControlSequenceOfOperation",
        0x001C => "SystemMode",
        0x0029 => "ThermostatRunningState",
        0xFFFC => "FeatureMap",
        0xFFFD => "ClusterRevision",
        0xFFF8 => "GeneratedCommandList",
        0xFFF9 => "AcceptedCommandList",
        0xFFFB => "AttributeList",
        _ => "Unknown",
    }
}

impl Handler for ZoneThermostatCluster {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;
        let attr_name = thermostat_attr_name(attr_id);

        info!(
            "Matter READ: ep={} cluster=0x{:04X}(Thermostat) attr=0x{:04X}({})",
            attr.endpoint_id, attr.cluster_id, attr_id, attr_name
        );

        // Get writer with dataver check
        let Some(writer) = reply.with_dataver(self.dataver())? else {
            // Dataver unchanged, no need to send data
            info!("  -> Thermostat: dataver unchanged, skipping");
            return Ok(());
        };

        let zone = self.zone_state().ok_or(ErrorCode::InvalidAction)?;

        match attr_id {
            // LocalTemperature - nullable i16
            x if x == attr::LOCAL_TEMPERATURE as u32 => {
                let temp = zone.get_local_temperature();
                if temp == thermos_matter::types::temperature::NULL_TEMPERATURE {
                    info!("  -> LocalTemperature: null (no reading)");
                    Reply::set(writer, Nullable::<i16>::none())?;
                } else {
                    let temp_c = temp as f32 / 100.0;
                    info!("  -> LocalTemperature: {} ({}°C)", temp, temp_c);
                    Reply::set(writer, Nullable::some(temp))?;
                }
            }

            // OccupiedHeatingSetpoint - i16
            x if x == attr::OCCUPIED_HEATING_SETPOINT as u32 => {
                let setpoint = zone.get_occupied_heating_setpoint();
                let setpoint_c = setpoint as f32 / 100.0;
                info!(
                    "  -> OccupiedHeatingSetpoint: {} ({}°C)",
                    setpoint, setpoint_c
                );
                Reply::set(writer, setpoint)?;
            }

            // AbsMinHeatSetpointLimit - i16 (hardware minimum, same as our min limit)
            x if x == attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32 => {
                info!(
                    "  -> AbsMinHeatSetpointLimit: {}",
                    defaults::MIN_HEAT_SETPOINT
                );
                Reply::set(writer, defaults::MIN_HEAT_SETPOINT)?;
            }

            // AbsMaxHeatSetpointLimit - i16 (hardware maximum, same as our max limit)
            x if x == attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32 => {
                info!(
                    "  -> AbsMaxHeatSetpointLimit: {}",
                    defaults::MAX_HEAT_SETPOINT
                );
                Reply::set(writer, defaults::MAX_HEAT_SETPOINT)?;
            }

            // MinHeatSetpointLimit - i16 (user-adjustable minimum)
            x if x == attr::MIN_HEAT_SETPOINT_LIMIT as u32 => {
                info!("  -> MinHeatSetpointLimit: {}", defaults::MIN_HEAT_SETPOINT);
                Reply::set(writer, defaults::MIN_HEAT_SETPOINT)?;
            }

            // MaxHeatSetpointLimit - i16 (user-adjustable maximum)
            x if x == attr::MAX_HEAT_SETPOINT_LIMIT as u32 => {
                info!("  -> MaxHeatSetpointLimit: {}", defaults::MAX_HEAT_SETPOINT);
                Reply::set(writer, defaults::MAX_HEAT_SETPOINT)?;
            }

            // OccupiedCoolingSetpoint - i16
            x if x == attr::OCCUPIED_COOLING_SETPOINT as u32 => {
                let setpoint = zone.get_occupied_cooling_setpoint();
                let setpoint_c = setpoint as f32 / 100.0;
                info!(
                    "  -> OccupiedCoolingSetpoint: {} ({}°C)",
                    setpoint, setpoint_c
                );
                Reply::set(writer, setpoint)?;
            }

            // AbsMinCoolSetpointLimit - i16
            x if x == attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32 => {
                info!(
                    "  -> AbsMinCoolSetpointLimit: {}",
                    defaults::MIN_COOL_SETPOINT
                );
                Reply::set(writer, defaults::MIN_COOL_SETPOINT)?;
            }

            // AbsMaxCoolSetpointLimit - i16
            x if x == attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32 => {
                info!(
                    "  -> AbsMaxCoolSetpointLimit: {}",
                    defaults::MAX_COOL_SETPOINT
                );
                Reply::set(writer, defaults::MAX_COOL_SETPOINT)?;
            }

            // MinCoolSetpointLimit - i16
            x if x == attr::MIN_COOL_SETPOINT_LIMIT as u32 => {
                info!("  -> MinCoolSetpointLimit: {}", defaults::MIN_COOL_SETPOINT);
                Reply::set(writer, defaults::MIN_COOL_SETPOINT)?;
            }

            // MaxCoolSetpointLimit - i16
            x if x == attr::MAX_COOL_SETPOINT_LIMIT as u32 => {
                info!("  -> MaxCoolSetpointLimit: {}", defaults::MAX_COOL_SETPOINT);
                Reply::set(writer, defaults::MAX_COOL_SETPOINT)?;
            }

            // ControlSequenceOfOperation - enum8 (CoolingAndHeating = 0x04)
            x if x == attr::CONTROL_SEQUENCE_OF_OPERATION as u32 => {
                let val = ControlSequenceOfOperationEnum::CoolingAndHeating as u8;
                info!(
                    "  -> ControlSequenceOfOperation: {} (CoolingAndHeating)",
                    val
                );
                Reply::set(writer, val)?;
            }

            // SystemMode - enum8
            x if x == attr::SYSTEM_MODE as u32 => {
                let mode = zone.get_system_mode();
                let val = match mode {
                    SystemMode::Off => SystemModeEnum::Off as u8,
                    SystemMode::Heat => SystemModeEnum::Heat as u8,
                    SystemMode::Cool => SystemModeEnum::Cool as u8,
                    _ => SystemModeEnum::Off as u8,
                };
                info!("  -> SystemMode: {} ({:?})", val, mode);
                Reply::set(writer, val)?;
            }

            // ThermostatRunningState - bitmap16
            x if x == attr::THERMOSTAT_RUNNING_STATE as u32 => {
                let is_heating = zone.is_heating();
                let bitmap: u16 = if is_heating {
                    RelayStateBitmap::HEAT.bits()
                } else {
                    0
                };
                info!(
                    "  -> ThermostatRunningState: 0x{:04X} (heating={})",
                    bitmap, is_heating
                );
                Reply::set(writer, bitmap)?;
            }

            // Global attributes - must handle these since we use raw Handler trait
            // FeatureMap (0xFFFC) - HEAT (0x01) + COOL (0x02) features
            0xFFFC => {
                let feature_map: u32 = 0x03; // HEAT (0x01) + COOL (0x02)
                info!("  -> FeatureMap: 0x{:08X} (HEAT+COOL)", feature_map);
                Reply::set(writer, feature_map)?;
            }

            // ClusterRevision (0xFFFD)
            0xFFFD => {
                let revision: u16 = 6; // Matter 1.3 thermostat cluster revision
                info!("  -> ClusterRevision: {}", revision);
                Reply::set(writer, revision)?;
            }

            // AttributeList (0xFFFB) - list of supported attribute IDs
            0xFFFB => {
                info!("  -> AttributeList: returning supported attributes");
                // Return array of attribute IDs we support
                let attrs: &[u32] = &[
                    attr::LOCAL_TEMPERATURE as u32,
                    attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32,
                    attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32,
                    attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32,
                    attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32,
                    attr::OCCUPIED_HEATING_SETPOINT as u32,
                    attr::OCCUPIED_COOLING_SETPOINT as u32,
                    attr::MIN_HEAT_SETPOINT_LIMIT as u32,
                    attr::MAX_HEAT_SETPOINT_LIMIT as u32,
                    attr::MIN_COOL_SETPOINT_LIMIT as u32,
                    attr::MAX_COOL_SETPOINT_LIMIT as u32,
                    attr::CONTROL_SEQUENCE_OF_OPERATION as u32,
                    attr::SYSTEM_MODE as u32,
                    attr::THERMOSTAT_RUNNING_STATE as u32,
                    0xFFF8, // GeneratedCommandList
                    0xFFF9, // AcceptedCommandList
                    0xFFFB, // AttributeList
                    0xFFFC, // FeatureMap
                    0xFFFD, // ClusterRevision
                ];
                Reply::set(writer, attrs)?;
            }

            // GeneratedCommandList (0xFFF8) - commands we generate (none for heating-only)
            0xFFF8 => {
                info!("  -> GeneratedCommandList: empty (no commands generated)");
                let cmds: &[u32] = &[];
                Reply::set(writer, cmds)?;
            }

            // AcceptedCommandList (0xFFF9) - commands we accept (none for heating-only, setpoint is via attribute write)
            0xFFF9 => {
                info!("  -> AcceptedCommandList: empty (setpoint via attribute write)");
                let cmds: &[u32] = &[];
                Reply::set(writer, cmds)?;
            }

            _ => {
                info!("  -> Thermostat: attribute 0x{:04X} not found", attr_id);
                return Err(ErrorCode::AttributeNotFound.into());
            }
        }

        Ok(())
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        let zone = self.zone_state().ok_or(ErrorCode::InvalidAction)?;

        match attr_id {
            // OccupiedHeatingSetpoint - writable
            x if x == attr::OCCUPIED_HEATING_SETPOINT as u32 => {
                let value: i16 = ctx.data().i16()?;

                // Validate range
                if value < defaults::MIN_HEAT_SETPOINT || value > defaults::MAX_HEAT_SETPOINT {
                    return Err(ErrorCode::ConstraintError.into());
                }

                zone.set_occupied_heating_setpoint(value)
                    .map_err(|_| Error::from(ErrorCode::ConstraintError))?;

                self.dataver_changed();
                ctx.notify_changed();
            }

            // OccupiedCoolingSetpoint - writable
            x if x == attr::OCCUPIED_COOLING_SETPOINT as u32 => {
                let value: i16 = ctx.data().i16()?;

                if value < defaults::MIN_COOL_SETPOINT || value > defaults::MAX_COOL_SETPOINT {
                    return Err(ErrorCode::ConstraintError.into());
                }

                zone.set_occupied_cooling_setpoint(value)
                    .map_err(|_| Error::from(ErrorCode::ConstraintError))?;

                self.dataver_changed();
                ctx.notify_changed();
            }

            // SystemMode - writable
            x if x == attr::SYSTEM_MODE as u32 => {
                let value: u8 = ctx.data().u8()?;

                let mode = match value {
                    x if x == SystemModeEnum::Off as u8 => SystemMode::Off,
                    x if x == SystemModeEnum::Heat as u8 => SystemMode::Heat,
                    x if x == SystemModeEnum::Cool as u8 => SystemMode::Cool,
                    _ => return Err(ErrorCode::ConstraintError.into()),
                };

                zone.set_system_mode(mode);
                self.dataver_changed();
                ctx.notify_changed();
            }

            // Read-only attributes
            x if x == attr::LOCAL_TEMPERATURE as u32
                || x == attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32
                || x == attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32
                || x == attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32
                || x == attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32
                || x == attr::MIN_HEAT_SETPOINT_LIMIT as u32
                || x == attr::MAX_HEAT_SETPOINT_LIMIT as u32
                || x == attr::MIN_COOL_SETPOINT_LIMIT as u32
                || x == attr::MAX_COOL_SETPOINT_LIMIT as u32
                || x == attr::CONTROL_SEQUENCE_OF_OPERATION as u32
                || x == attr::THERMOSTAT_RUNNING_STATE as u32 =>
            {
                return Err(ErrorCode::UnsupportedAccess.into());
            }

            _ => {
                return Err(ErrorCode::AttributeNotFound.into());
            }
        }

        Ok(())
    }

    // No commands for heating-only thermostat (setpoint is via attribute write)
    // The default invoke() returns CommandNotFound which is correct
}

// Mark as non-blocking since we only do atomic operations
impl NonBlockingHandler for ZoneThermostatCluster {}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert zone index to Matter endpoint ID
pub fn zone_to_endpoint(zone_index: usize) -> u16 {
    FIRST_ZONE_ENDPOINT + zone_index as u16
}

/// Convert Matter endpoint ID to zone index
pub fn endpoint_to_zone(endpoint_id: u16) -> Option<usize> {
    if endpoint_id >= FIRST_ZONE_ENDPOINT {
        let index = (endpoint_id - FIRST_ZONE_ENDPOINT) as usize;
        if index < NUM_ZONES {
            return Some(index);
        }
    }
    None
}

// =============================================================================
// Thermostat Cluster Definition
// =============================================================================

/// Thermostat cluster attributes for heating and cooling mode
///
/// Based on Matter spec section 4.3 (Thermostat Cluster)
const THERMOSTAT_ATTRIBUTES: &[Attribute] = &[
    // LocalTemperature (0x0000) - nullable i16, read-only
    Attribute::new(attr::LOCAL_TEMPERATURE as u32, Access::RV, Quality::X),
    // AbsMinHeatSetpointLimit (0x0003) - i16, read-only
    Attribute::new(
        attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32,
        Access::RV,
        Quality::F,
    ),
    // AbsMaxHeatSetpointLimit (0x0004) - i16, read-only
    Attribute::new(
        attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32,
        Access::RV,
        Quality::F,
    ),
    // AbsMinCoolSetpointLimit (0x0005) - i16, read-only
    Attribute::new(
        attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32,
        Access::RV,
        Quality::F,
    ),
    // AbsMaxCoolSetpointLimit (0x0006) - i16, read-only
    Attribute::new(
        attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32,
        Access::RV,
        Quality::F,
    ),
    // OccupiedCoolingSetpoint (0x0011) - i16, read-write
    Attribute::new(
        attr::OCCUPIED_COOLING_SETPOINT as u32,
        Access::RWVM,
        Quality::SN,
    ),
    // OccupiedHeatingSetpoint (0x0012) - i16, read-write
    Attribute::new(
        attr::OCCUPIED_HEATING_SETPOINT as u32,
        Access::RWVM,
        Quality::SN,
    ),
    // MinHeatSetpointLimit (0x0015) - i16, read-only
    Attribute::new(attr::MIN_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MaxHeatSetpointLimit (0x0016) - i16, read-only
    Attribute::new(attr::MAX_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MinCoolSetpointLimit (0x0017) - i16, read-only
    Attribute::new(attr::MIN_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MaxCoolSetpointLimit (0x0018) - i16, read-only
    Attribute::new(attr::MAX_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // ControlSequenceOfOperation (0x001B) - enum8, read-only
    Attribute::new(
        attr::CONTROL_SEQUENCE_OF_OPERATION as u32,
        Access::RV,
        Quality::N,
    ),
    // SystemMode (0x001C) - enum8, read-write
    Attribute::new(attr::SYSTEM_MODE as u32, Access::RWVM, Quality::SN),
    // ThermostatRunningState (0x0029) - bitmap16, read-only
    Attribute::new(
        attr::THERMOSTAT_RUNNING_STATE as u32,
        Access::RV,
        Quality::NONE,
    ),
];

/// Thermostat cluster metadata for Matter node definition
pub const THERMOSTAT_CLUSTER: Cluster<'static> = Cluster::new(
    CLUSTER_THERMOSTAT,    // cluster ID 0x0201
    6,                     // revision (Matter 1.3 thermostat spec)
    0x03,                  // feature map: Heating + Cooling features
    THERMOSTAT_ATTRIBUTES, // attributes
    &[],                   // commands
    |_, _, _| true,        // with_attrs: include all
    |_, _, _| true,        // with_cmds: include all
);

// =============================================================================
// Matter Device Configuration
// =============================================================================

/// Device details for Matter commissioning
const DEV_DET: BasicInfoConfig = BasicInfoConfig {
    vid: MATTER_VENDOR_ID,
    pid: MATTER_PRODUCT_ID,
    hw_ver: 1,
    hw_ver_str: "1",
    sw_ver: 1,
    sw_ver_str: "1",
    serial_no: "thermos-001",
    product_name: MATTER_DEVICE_NAME,
    vendor_name: "Thermos",
    device_name: MATTER_DEVICE_NAME,
    ..BasicInfoConfig::new()
};

/// Commissioning data for Matter pairing
const DEV_COMM: BasicCommData = BasicCommData {
    password: MATTER_PASSCODE,
    discriminator: MATTER_DISCRIMINATOR,
};

// =============================================================================
// Matter Stack Configuration
// =============================================================================

/// Bump allocator size for Matter stack futures
///
/// This is the memory used by rs-matter-stack's internal bump allocator
/// for its async futures. Memory breakdown:
/// - Base WiFi+BLE drivers: ~16.5KB
/// - BLE commissioning mode: ~6KB additional
/// - Matter responder handlers (4x): ~4.5KB
/// - MQTT TCP sockets + client: ~10KB
/// - Headroom: ~3KB
/// Total: ~40KB
const BUMP_SIZE: usize = 40000;

/// Epoch function for Matter timestamps
fn epoch() -> core::time::Duration {
    rs_matter_embassy::epoch::epoch()
}

/// Static allocation macro for Matter stack
///
/// Using StaticCell for no_std embedded systems.
macro_rules! mk_static {
    ($t:ty) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit()
    }};
}

// =============================================================================
// Matter Task Entry Point
// =============================================================================

/// Run the Matter stack with a single zone thermostat endpoint
///
/// This is the main entry point for Matter integration. It:
/// 1. Initializes the Matter stack with device credentials
/// 2. Creates a handler chain with thermostat cluster for zone 0
/// 3. Runs the Matter stack with WiFi and BLE commissioning
/// 4. Optionally runs a user task (e.g., MQTT) alongside Matter
///
/// # Arguments
///
/// * `controller` - The esp-radio controller for WiFi+BLE
/// * `wifi` - The WiFi peripheral
/// * `bt` - The Bluetooth peripheral
/// * `user_task` - User task that runs alongside Matter (can be `()` to disable)
///
/// # Note
///
/// This function never returns under normal operation. It runs the Matter
/// stack indefinitely, handling commissioning and attribute read/write requests.
pub async fn matter_task<U>(
    controller: &'static EspRadioController<'static>,
    wifi: WIFI<'static>,
    bt: BT<'static>,
    user_task: U,
) where
    U: rs_matter_embassy::stack::UserTask,
{
    info!("Matter: Starting task...");

    // Note: esp_init_rand is called early in main.rs before esp_rtos::start
    // per rs-matter-embassy example requirements

    // Allocate Matter stack statically (~35-50KB)
    let stack = mk_static!(EmbassyWifiMatterStack::<BUMP_SIZE, ()>).init_with(
        EmbassyWifiMatterStack::init(&DEV_DET, DEV_COMM, &TEST_DEV_ATT, epoch, esp_rand),
    );

    info!("Matter: Stack initialized");
    info!(
        "Matter: Discriminator={}, Passcode={}",
        MATTER_DISCRIMINATOR, MATTER_PASSCODE
    );

    // Create thermostat handler for zone 0 (endpoint 1)
    let thermostat = ZoneThermostatCluster::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    // Create identify handler for zone 0 (endpoint 1)
    // Required by HomeKit for proper device identification
    let identify = IdentifyHandler::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    // Create thermostat UI config handler for zone 0 (endpoint 1)
    // Controls temperature display format (Celsius/Fahrenheit)
    let thermostat_ui = ThermostatUiConfigHandler::new(Dataver::new_rand(stack.matter().rand()));

    // Create binding handler for zone 0 (endpoint 1)
    // Required by HomeKit for external sensor integration (Eve Thermo has this)
    let binding = BindingHandler::new(Dataver::new_rand(stack.matter().rand()));

    // Build handler chain: identify + binding + thermostat UI + thermostat + descriptor clusters on endpoint 1
    // Wrap with Async() to convert NonBlockingHandler to AsyncHandler
    let handler = EmptyHandler
        // Identify cluster on endpoint 1 (required by HomeKit)
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_IDENTIFY)),
            Async(&identify),
        )
        // Binding cluster on endpoint 1 (required by HomeKit for external sensor support)
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_BINDING)),
            Async(&binding),
        )
        // Thermostat UI Configuration cluster on endpoint 1
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(CLUSTER_THERMOSTAT_UI_CONFIG),
            ),
            Async(&thermostat_ui),
        )
        // Thermostat cluster on endpoint 1
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_THERMOSTAT)),
            Async(&thermostat),
        )
        // Descriptor cluster on endpoint 1 (required by Matter spec)
        // Wrapped with logging to trace what HomeKit reads
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(desc::DescHandler::CLUSTER.id),
            ),
            Async(DescriptorLoggingHandler::new(
                desc::DescHandler::new(Dataver::new_rand(stack.matter().rand())).adapt(),
                FIRST_ZONE_ENDPOINT,
            )),
        );

    info!("Matter: Handler chain created");

    // Log the exact device configuration being advertised
    info!("Matter: Device Configuration:");
    info!(
        "  VID: 0x{:04X}, PID: 0x{:04X}",
        MATTER_VENDOR_ID, MATTER_PRODUCT_ID
    );
    info!("  Device Name: {}", MATTER_DEVICE_NAME);
    info!("  Endpoint 0 (Root): device_type=0x0016 (Root Node)");
    info!(
        "  Endpoint 1 (Thermostat): device_type=0x{:04X} rev={}",
        DEV_TYPE_THERMOSTAT.dtype, DEV_TYPE_THERMOSTAT.drev
    );
    info!("  Endpoint 1 clusters: Identify(0x03), Binding(0x1E), ThermostatUI(0x204), Descriptor(0x1D), Thermostat(0x201) [HEAT+COOL]");

    // Create persister (using dummy store for now - TODO: implement NVS persistence)
    let persist = stack
        .create_persist_with_comm_window(DummyKvBlobStore)
        .await
        .unwrap();

    info!("Matter: Persist created, starting stack...");

    // Define the Matter node with root endpoint + thermostat endpoint
    const NODE: Node = Node {
        id: 0,
        endpoints: &[
            EmbassyWifiMatterStack::<0, ()>::root_endpoint(),
            Endpoint {
                id: FIRST_ZONE_ENDPOINT,
                device_types: devices!(DEV_TYPE_THERMOSTAT),
                clusters: clusters!(
                    IDENTIFY_CLUSTER,
                    BINDING_CLUSTER,
                    THERMOSTAT_UI_CONFIG_CLUSTER,
                    desc::DescHandler::CLUSTER,
                    THERMOSTAT_CLUSTER
                ),
            },
        ],
    };

    // Run Matter stack with concurrent commissioning (BLE + WiFi simultaneously)
    // This is required for Home Assistant and Alexa compatibility.
    // Note: ESP32-C6 has a single radio, so BLE and WiFi time-share.
    // User task (e.g., MqttBridge) runs alongside Matter using the WiFi connection.
    let matter = pin!(stack.run_coex(
        EmbassyWifi::new(EspWifiDriver::new(controller, wifi, bt), stack),
        &persist,
        (NODE, handler),
        user_task,
    ));

    info!("Matter: Running...");
    matter.await.unwrap();
}

/// Run the Matter stack with dual-radio (C6 WiFi + H2 BLE)
///
/// This variant uses an external BLE controller over UART HCI instead of
/// the ESP32-C6's internal BLE, avoiding radio time-sharing issues.
#[cfg(feature = "external-ble")]
pub async fn matter_task_dual_radio<D, U>(driver: D, user_task: U)
where
    D: WifiCoexDriver + WifiDriver + BleDriver,
    U: rs_matter_embassy::stack::UserTask,
{
    info!("Matter: Starting dual-radio task (C6 WiFi + H2 BLE)...");

    let stack = mk_static!(EmbassyWifiMatterStack::<BUMP_SIZE, ()>).init_with(
        EmbassyWifiMatterStack::init(&DEV_DET, DEV_COMM, &TEST_DEV_ATT, epoch, esp_rand),
    );

    info!("Matter: Stack initialized (dual-radio mode)");
    info!(
        "Matter: Discriminator={}, Passcode={}",
        MATTER_DISCRIMINATOR, MATTER_PASSCODE
    );

    let thermostat = ZoneThermostatCluster::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    let identify = IdentifyHandler::new(
        FIRST_ZONE_ENDPOINT,
        Dataver::new_rand(stack.matter().rand()),
    );

    let thermostat_ui = ThermostatUiConfigHandler::new(Dataver::new_rand(stack.matter().rand()));

    let binding = BindingHandler::new(Dataver::new_rand(stack.matter().rand()));

    let handler = EmptyHandler
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_IDENTIFY)),
            Async(&identify),
        )
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_BINDING)),
            Async(&binding),
        )
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(CLUSTER_THERMOSTAT_UI_CONFIG),
            ),
            Async(&thermostat_ui),
        )
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_THERMOSTAT)),
            Async(&thermostat),
        )
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(desc::DescHandler::CLUSTER.id),
            ),
            Async(DescriptorLoggingHandler::new(
                desc::DescHandler::new(Dataver::new_rand(stack.matter().rand())).adapt(),
                FIRST_ZONE_ENDPOINT,
            )),
        );

    // Log the exact device configuration being advertised
    info!("Matter: Device Configuration:");
    info!(
        "  VID: 0x{:04X}, PID: 0x{:04X}",
        MATTER_VENDOR_ID, MATTER_PRODUCT_ID
    );
    info!("  Device Name: {}", MATTER_DEVICE_NAME);
    info!("  Endpoint 0 (Root): device_type=0x0016 (Root Node)");
    info!(
        "  Endpoint 1 (Thermostat): device_type=0x{:04X} rev={}",
        DEV_TYPE_THERMOSTAT.dtype, DEV_TYPE_THERMOSTAT.drev
    );
    info!("  Endpoint 1 clusters: Identify(0x03), Binding(0x1E), ThermostatUI(0x204), Descriptor(0x1D), Thermostat(0x201) [HEAT+COOL]");

    let persist = stack
        .create_persist_with_comm_window(DummyKvBlobStore)
        .await
        .unwrap();

    const NODE: Node = Node {
        id: 0,
        endpoints: &[
            EmbassyWifiMatterStack::<0, ()>::root_endpoint(),
            Endpoint {
                id: FIRST_ZONE_ENDPOINT,
                device_types: devices!(DEV_TYPE_THERMOSTAT),
                clusters: clusters!(
                    IDENTIFY_CLUSTER,
                    BINDING_CLUSTER,
                    THERMOSTAT_UI_CONFIG_CLUSTER,
                    desc::DescHandler::CLUSTER,
                    THERMOSTAT_CLUSTER
                ),
            },
        ],
    };

    let matter = pin!(stack.run_coex(
        EmbassyWifi::new(driver, stack),
        &persist,
        (NODE, handler),
        user_task,
    ));

    info!("Matter: Running with dual-radio (C6 WiFi + H2 BLE)...");
    matter.await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_endpoint_conversion() {
        assert_eq!(zone_to_endpoint(0), 1);
        assert_eq!(zone_to_endpoint(1), 2);
        assert_eq!(zone_to_endpoint(6), 7);

        assert_eq!(endpoint_to_zone(1), Some(0));
        assert_eq!(endpoint_to_zone(2), Some(1));
        assert_eq!(endpoint_to_zone(0), None); // Root endpoint
    }

    #[test]
    fn test_thermostat_cluster_definition() {
        assert_eq!(THERMOSTAT_CLUSTER.id, CLUSTER_THERMOSTAT);
        assert_eq!(THERMOSTAT_CLUSTER.revision, 6);
        assert_eq!(THERMOSTAT_CLUSTER.feature_map, 0x03); // Heating + Cooling features
        assert!(!THERMOSTAT_ATTRIBUTES.is_empty());
    }
}
