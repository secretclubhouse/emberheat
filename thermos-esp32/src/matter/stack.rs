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
use rs_matter_embassy::rand::esp::{esp_init_rand, esp_rand};
use rs_matter_embassy::stack::persist::DummyKvBlobStore;
use rs_matter_embassy::wireless::esp::EspWifiDriver;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiMatterStack};

use esp_hal::peripherals::{BT, WIFI};
use esp_radio::Controller as EspRadioController;

use log::info;

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

// =============================================================================
// Thermostat Attribute IDs (heating-only subset)
// =============================================================================

/// Thermostat attribute IDs from Matter specification
pub mod attr {
    /// Current temperature in 0.01°C units (nullable i16)
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    /// Target heating temperature in 0.01°C units (i16)
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    /// Minimum allowed setpoint (i16)
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    /// Maximum allowed setpoint (i16)
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
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

impl Handler for ZoneThermostatCluster {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let attr_id = attr.attr_id;

        // Get writer with dataver check
        let Some(writer) = reply.with_dataver(self.dataver())? else {
            // Dataver unchanged, no need to send data
            return Ok(());
        };

        let zone = self.zone_state().ok_or(ErrorCode::InvalidAction)?;

        match attr_id {
            // LocalTemperature - nullable i16
            x if x == attr::LOCAL_TEMPERATURE as u32 => {
                let temp = zone.get_local_temperature();
                if temp == thermos_matter::types::temperature::NULL_TEMPERATURE {
                    Reply::set(writer, Nullable::<i16>::none())?;
                } else {
                    Reply::set(writer, Nullable::some(temp))?;
                }
            }

            // OccupiedHeatingSetpoint - i16
            x if x == attr::OCCUPIED_HEATING_SETPOINT as u32 => {
                let setpoint = zone.get_occupied_heating_setpoint();
                Reply::set(writer, setpoint)?;
            }

            // MinHeatSetpointLimit - i16
            x if x == attr::MIN_HEAT_SETPOINT_LIMIT as u32 => {
                Reply::set(writer, defaults::MIN_HEAT_SETPOINT)?;
            }

            // MaxHeatSetpointLimit - i16
            x if x == attr::MAX_HEAT_SETPOINT_LIMIT as u32 => {
                Reply::set(writer, defaults::MAX_HEAT_SETPOINT)?;
            }

            // ControlSequenceOfOperation - enum8 (always HeatingOnly = 0x02)
            x if x == attr::CONTROL_SEQUENCE_OF_OPERATION as u32 => {
                let val = ControlSequenceOfOperationEnum::HeatingOnly as u8;
                Reply::set(writer, val)?;
            }

            // SystemMode - enum8
            x if x == attr::SYSTEM_MODE as u32 => {
                let mode = zone.get_system_mode();
                let val = match mode {
                    SystemMode::Off => SystemModeEnum::Off as u8,
                    SystemMode::Heat => SystemModeEnum::Heat as u8,
                    _ => SystemModeEnum::Off as u8, // Default to Off for unsupported
                };
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
                Reply::set(writer, bitmap)?;
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

            // SystemMode - writable
            x if x == attr::SYSTEM_MODE as u32 => {
                let value: u8 = ctx.data().u8()?;

                let mode = match value {
                    x if x == SystemModeEnum::Off as u8 => SystemMode::Off,
                    x if x == SystemModeEnum::Heat as u8 => SystemMode::Heat,
                    _ => return Err(ErrorCode::ConstraintError.into()),
                };

                zone.set_system_mode(mode);
                self.dataver_changed();
                ctx.notify_changed();
            }

            // Read-only attributes
            x if x == attr::LOCAL_TEMPERATURE as u32
                || x == attr::MIN_HEAT_SETPOINT_LIMIT as u32
                || x == attr::MAX_HEAT_SETPOINT_LIMIT as u32
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

/// Thermostat cluster attributes for heating-only mode
///
/// Based on Matter spec section 4.3 (Thermostat Cluster)
const THERMOSTAT_ATTRIBUTES: &[Attribute] = &[
    // LocalTemperature (0x0000) - nullable i16, read-only
    Attribute::new(attr::LOCAL_TEMPERATURE as u32, Access::RV, Quality::X),
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
    0x01,                  // feature map: Heating feature enabled
    THERMOSTAT_ATTRIBUTES, // attributes
    &[],                   // commands (none for heating-only)
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
/// for its async futures. 16.5KB is sufficient for WiFi+BLE coexistence.
const BUMP_SIZE: usize = 16500;

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
///
/// # Arguments
///
/// * `wifi` - The WiFi peripheral
/// * `bt` - The Bluetooth peripheral
/// * `init` - The esp-radio initialization handle
///
/// # Note
///
/// This function never returns under normal operation. It runs the Matter
/// stack indefinitely, handling commissioning and attribute read/write requests.
pub async fn matter_task(
    controller: &'static EspRadioController<'static>,
    wifi: WIFI<'static>,
    bt: BT<'static>,
) {
    info!("Matter: Starting task...");

    // Initialize random number generator for Matter crypto
    esp_init_rand(esp_hal::rng::Rng::new());

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

    // Build handler chain: descriptor + thermostat clusters on endpoint 1
    // Wrap with Async() to convert NonBlockingHandler to AsyncHandler
    let handler = EmptyHandler
        // Thermostat cluster on endpoint 1
        .chain(
            EpClMatcher::new(Some(FIRST_ZONE_ENDPOINT), Some(CLUSTER_THERMOSTAT)),
            Async(&thermostat),
        )
        // Descriptor cluster on endpoint 1 (required by Matter spec)
        .chain(
            EpClMatcher::new(
                Some(FIRST_ZONE_ENDPOINT),
                Some(desc::DescHandler::CLUSTER.id),
            ),
            Async(desc::DescHandler::new(Dataver::new_rand(stack.matter().rand())).adapt()),
        );

    info!("Matter: Handler chain created");

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
                clusters: clusters!(desc::DescHandler::CLUSTER, THERMOSTAT_CLUSTER),
            },
        ],
    };

    // Run Matter stack with WiFi+BLE coexistence (required for Alexa)
    let matter = pin!(stack.run_coex(
        EmbassyWifi::new(EspWifiDriver::new(controller, wifi, bt), stack),
        &persist,
        (NODE, handler),
        (), // No user future
    ));

    info!("Matter: Running...");
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
        assert_eq!(THERMOSTAT_CLUSTER.feature_map, 0x01); // Heating feature
        assert!(!THERMOSTAT_ATTRIBUTES.is_empty());
    }
}
