/**
 * @file matter_wrapper.h
 * @brief C wrapper for Matter (Connected Home IP) C++ API
 *
 * This header provides a C-compatible interface for integrating the Matter
 * protocol stack (from project-chip/connectedhomeip) with Rust via FFI.
 *
 * The wrapper focuses on thermostat functionality needed for the thermos-rust
 * project, providing a minimal but complete interface for:
 * - Matter device initialization and lifecycle
 * - Thermostat cluster attribute handling
 * - Commissioning and pairing
 * - Network management (Thread/WiFi)
 */

#ifndef MATTER_WRAPPER_H
#define MATTER_WRAPPER_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Return codes */
typedef enum {
    MATTER_SUCCESS = 0,
    MATTER_ERROR_INIT_FAILED = 1,
    MATTER_ERROR_INVALID_PARAM = 2,
    MATTER_ERROR_NOT_INITIALIZED = 3,
    MATTER_ERROR_NETWORK_FAILED = 4,
    MATTER_ERROR_COMMISSIONING_FAILED = 5,
    MATTER_ERROR_CLUSTER_FAILED = 6,
    MATTER_ERROR_UNKNOWN = 99
} MatterErrorCode;

/* Opaque handle types */
typedef struct MatterDevice MatterDevice;
typedef struct MatterThermostat MatterThermostat;

/* System mode values (Matter spec) */
typedef enum {
    MATTER_THERMOSTAT_MODE_OFF = 0x00,
    MATTER_THERMOSTAT_MODE_AUTO = 0x01,
    MATTER_THERMOSTAT_MODE_COOL = 0x03,
    MATTER_THERMOSTAT_MODE_HEAT = 0x04,
    MATTER_THERMOSTAT_MODE_EMERGENCY_HEAT = 0x05,
    MATTER_THERMOSTAT_MODE_PRECOOLING = 0x06,
    MATTER_THERMOSTAT_MODE_FAN_ONLY = 0x07
} MatterThermostatMode;

/* Running state flags (Matter spec) */
typedef enum {
    MATTER_THERMOSTAT_RUNNING_IDLE = 0x0000,
    MATTER_THERMOSTAT_RUNNING_HEAT = 0x0001,
    MATTER_THERMOSTAT_RUNNING_COOL = 0x0002,
    MATTER_THERMOSTAT_RUNNING_FAN = 0x0004,
    MATTER_THERMOSTAT_RUNNING_HEAT_STAGE_2 = 0x0008,
    MATTER_THERMOSTAT_RUNNING_COOL_STAGE_2 = 0x0010,
    MATTER_THERMOSTAT_RUNNING_FAN_STAGE_2 = 0x0020,
    MATTER_THERMOSTAT_RUNNING_FAN_STAGE_3 = 0x0040
} MatterThermostatRunningState;

/* Device configuration */
typedef struct {
    uint16_t vendor_id;           /* Matter vendor ID */
    uint16_t product_id;          /* Matter product ID */
    const char* device_name;      /* Device name for commissioning */
    const char* serial_number;    /* Serial number */
    uint16_t discriminator;       /* Commissioning discriminator (12-bit) */
    uint32_t setup_passcode;      /* Setup passcode for pairing */
    uint16_t port;                /* UDP port (default 5540) */
} MatterDeviceConfig;

/* Thermostat configuration */
typedef struct {
    uint16_t endpoint_id;         /* Endpoint ID for this thermostat */
    const char* zone_name;        /* Zone name */
    int16_t min_heat_setpoint;    /* Min heating setpoint (centi-°C) */
    int16_t max_heat_setpoint;    /* Max heating setpoint (centi-°C) */
    int16_t min_cool_setpoint;    /* Min cooling setpoint (centi-°C) */
    int16_t max_cool_setpoint;    /* Max cooling setpoint (centi-°C) */
} MatterThermostatConfig;

/* Callback types for thermostat events */
typedef void (*MatterThermostatSetpointChangedCallback)(
    void* context,
    uint16_t endpoint_id,
    int16_t heating_setpoint,
    int16_t cooling_setpoint
);

typedef void (*MatterThermostatModeChangedCallback)(
    void* context,
    uint16_t endpoint_id,
    MatterThermostatMode mode
);

/* Commissioning callbacks */
typedef void (*MatterCommissioningCompleteCallback)(
    void* context,
    bool success
);

/* ============================================================================
 * Device Lifecycle Functions
 * ========================================================================== */

/**
 * Initialize the Matter stack
 *
 * This must be called before any other Matter functions. It initializes
 * the Matter protocol stack, sets up event handling, and prepares for
 * device commissioning.
 *
 * @param config Device configuration
 * @return MatterDevice handle on success, NULL on failure
 */
MatterDevice* matter_device_init(const MatterDeviceConfig* config);

/**
 * Shutdown the Matter device and cleanup resources
 *
 * @param device Device handle
 */
void matter_device_shutdown(MatterDevice* device);

/**
 * Run the Matter event loop (non-blocking)
 *
 * This should be called periodically (e.g., every 100ms) to process
 * Matter protocol events. It returns immediately after processing
 * pending events.
 *
 * @param device Device handle
 * @return Error code
 */
MatterErrorCode matter_device_run_event_loop(MatterDevice* device);

/**
 * Check if device is commissioned (paired)
 *
 * @param device Device handle
 * @return true if commissioned, false otherwise
 */
bool matter_device_is_commissioned(MatterDevice* device);

/* ============================================================================
 * Commissioning Functions
 * ========================================================================== */

/**
 * Get commissioning QR code
 *
 * @param device Device handle
 * @param buffer Buffer to store QR code string
 * @param buffer_size Size of buffer
 * @return Error code
 */
MatterErrorCode matter_device_get_qr_code(
    MatterDevice* device,
    char* buffer,
    size_t buffer_size
);

/**
 * Get manual pairing code
 *
 * @param device Device handle
 * @param buffer Buffer to store pairing code
 * @param buffer_size Size of buffer
 * @return Error code
 */
MatterErrorCode matter_device_get_pairing_code(
    MatterDevice* device,
    char* buffer,
    size_t buffer_size
);

/**
 * Set commissioning complete callback
 *
 * @param device Device handle
 * @param callback Callback function
 * @param context User context passed to callback
 */
void matter_device_set_commissioning_callback(
    MatterDevice* device,
    MatterCommissioningCompleteCallback callback,
    void* context
);

/* ============================================================================
 * Thermostat Functions
 * ========================================================================== */

/**
 * Create a thermostat accessory on the device
 *
 * @param device Device handle
 * @param config Thermostat configuration
 * @return MatterThermostat handle on success, NULL on failure
 */
MatterThermostat* matter_thermostat_create(
    MatterDevice* device,
    const MatterThermostatConfig* config
);

/**
 * Destroy a thermostat accessory
 *
 * @param thermostat Thermostat handle
 */
void matter_thermostat_destroy(MatterThermostat* thermostat);

/**
 * Update local temperature reading
 *
 * This should be called whenever the temperature sensor provides a new reading.
 *
 * @param thermostat Thermostat handle
 * @param temperature Temperature in centi-degrees Celsius (e.g., 2050 = 20.5°C)
 * @return Error code
 */
MatterErrorCode matter_thermostat_set_local_temperature(
    MatterThermostat* thermostat,
    int16_t temperature
);

/**
 * Get current heating setpoint
 *
 * @param thermostat Thermostat handle
 * @param setpoint Output: heating setpoint in centi-degrees Celsius
 * @return Error code
 */
MatterErrorCode matter_thermostat_get_heating_setpoint(
    MatterThermostat* thermostat,
    int16_t* setpoint
);

/**
 * Get current cooling setpoint
 *
 * @param thermostat Thermostat handle
 * @param setpoint Output: cooling setpoint in centi-degrees Celsius
 * @return Error code
 */
MatterErrorCode matter_thermostat_get_cooling_setpoint(
    MatterThermostat* thermostat,
    int16_t* setpoint
);

/**
 * Get system mode
 *
 * @param thermostat Thermostat handle
 * @param mode Output: system mode
 * @return Error code
 */
MatterErrorCode matter_thermostat_get_system_mode(
    MatterThermostat* thermostat,
    MatterThermostatMode* mode
);

/**
 * Set running state
 *
 * This should be called to update the actual running state of the HVAC system
 * (e.g., when the relay turns on/off).
 *
 * @param thermostat Thermostat handle
 * @param state Running state flags
 * @return Error code
 */
MatterErrorCode matter_thermostat_set_running_state(
    MatterThermostat* thermostat,
    uint16_t state
);

/**
 * Set setpoint changed callback
 *
 * This callback is invoked when a Matter controller (e.g., HomeKit) changes
 * the heating or cooling setpoint.
 *
 * @param thermostat Thermostat handle
 * @param callback Callback function
 * @param context User context passed to callback
 */
void matter_thermostat_set_setpoint_callback(
    MatterThermostat* thermostat,
    MatterThermostatSetpointChangedCallback callback,
    void* context
);

/**
 * Set mode changed callback
 *
 * This callback is invoked when a Matter controller changes the system mode.
 *
 * @param thermostat Thermostat handle
 * @param callback Callback function
 * @param context User context passed to callback
 */
void matter_thermostat_set_mode_callback(
    MatterThermostat* thermostat,
    MatterThermostatModeChangedCallback callback,
    void* context
);

/* ============================================================================
 * Utility Functions
 * ========================================================================== */

/**
 * Get error message for error code
 *
 * @param error Error code
 * @return Error message string (static, do not free)
 */
const char* matter_get_error_message(MatterErrorCode error);

/**
 * Get Matter stack version
 *
 * @return Version string (static, do not free)
 */
const char* matter_get_version(void);

#ifdef __cplusplus
}
#endif

#endif /* MATTER_WRAPPER_H */
