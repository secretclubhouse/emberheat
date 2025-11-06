/**
 * @file matter_wrapper.cpp
 * @brief C wrapper implementation for Matter (Connected Home IP) C++ API
 *
 * This implementation wraps the Matter C++ SDK to provide a C-compatible
 * interface for Rust FFI integration.
 */

#include "matter_wrapper.h"

#include <cstring>
#include <cstdio>
#include <cstdlib>  /* for free() */
#include <memory>
#include <map>

// Matter SDK includes
// Conditionally include Matter SDK headers when available
#ifdef MATTER_SDK_AVAILABLE
#include <app/server/Server.h>
#include <app/clusters/thermostat-server/thermostat-server.h>
#include <app/util/af.h>
#include <app/util/attribute-storage.h>
#include <platform/CHIPDeviceLayer.h>
#include <platform/PlatformManager.h>
#include <setup_payload/QRCodeSetupPayloadGenerator.h>
#include <setup_payload/ManualSetupPayloadGenerator.h>
#include <setup_payload/SetupPayload.h>
#include <lib/core/CHIPError.h>
#include <lib/support/logging/CHIPLogging.h>

using namespace chip;
using namespace chip::DeviceLayer;
using namespace chip::app;
using namespace chip::app::Clusters;
#endif

/* ============================================================================
 * Internal Structures
 * ========================================================================== */

struct MatterDevice {
    MatterDeviceConfig config;
    bool initialized;
    bool commissioned;
    MatterCommissioningCompleteCallback commissioning_callback;
    void* commissioning_context;
    std::map<uint16_t, MatterThermostat*> thermostats;
};

struct MatterThermostat {
    MatterDevice* device;
    MatterThermostatConfig config;
    int16_t local_temperature;
    int16_t heating_setpoint;
    int16_t cooling_setpoint;
    MatterThermostatMode system_mode;
    uint16_t running_state;
    MatterThermostatSetpointChangedCallback setpoint_callback;
    void* setpoint_context;
    MatterThermostatModeChangedCallback mode_callback;
    void* mode_context;
};

// Global device pointer for callback access
// This is set when the first device is initialized
static MatterDevice* g_matter_device = nullptr;

/* ============================================================================
 * Internal Helper Functions
 * ========================================================================== */

static const char* error_messages[] = {
    "Success",
    "Initialization failed",
    "Invalid parameter",
    "Not initialized",
    "Network failed",
    "Commissioning failed",
    "Cluster operation failed",
    "Unknown error"
};

/* ============================================================================
 * Device Lifecycle Functions
 * ========================================================================== */

MatterDevice* matter_device_init(const MatterDeviceConfig* config) {
    if (!config) {
        return nullptr;
    }

    // Allocate device structure
    MatterDevice* device = new MatterDevice();
    if (!device) {
        return nullptr;
    }

    // Copy configuration
    device->config = *config;
    device->config.device_name = config->device_name ? ::strdup(config->device_name) : nullptr;
    device->config.serial_number = config->serial_number ? ::strdup(config->serial_number) : nullptr;
    device->initialized = false;
    device->commissioned = false;
    device->commissioning_callback = nullptr;
    device->commissioning_context = nullptr;

    // Initialize Matter stack
#ifdef MATTER_SDK_AVAILABLE
    CHIP_ERROR err = CHIP_NO_ERROR;

    // Initialize platform memory
    err = Platform::MemoryInit();
    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Matter MemoryInit failed: %s\n", ErrorStr(err));
        delete device;
        return nullptr;
    }

    // Initialize CHIP stack
    err = PlatformMgr().InitChipStack();
    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Matter InitChipStack failed: %s\n", ErrorStr(err));
        Platform::MemoryShutdown();
        delete device;
        return nullptr;
    }

    // Set BLE device name for commissioning
    if (config->device_name) {
        err = ConnectivityMgr().SetBLEDeviceName(config->device_name);
        if (err != CHIP_NO_ERROR) {
            fprintf(stderr, "Matter SetBLEDeviceName failed: %s\n", ErrorStr(err));
            // Non-fatal, continue
        }
    }

    // Initialize the Matter server
    static ServerInitParams initParams;
    err = Server::GetInstance().Init(initParams);
    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Matter Server Init failed: %s\n", ErrorStr(err));
        PlatformMgr().Shutdown();
        Platform::MemoryShutdown();
        delete device;
        return nullptr;
    }

    device->initialized = true;

    // Set global device pointer for callback access
    if (!g_matter_device) {
        g_matter_device = device;
    }

    fprintf(stderr, "Matter device initialized: %s (Vendor: 0x%04X, Product: 0x%04X)\n",
            device->config.device_name,
            device->config.vendor_id,
            device->config.product_id);
#else
    // Stub implementation when SDK not available
    device->initialized = true;

    // Set global device pointer for callback access
    if (!g_matter_device) {
        g_matter_device = device;
    }

    fprintf(stderr, "Matter device initialized (STUB): %s (Vendor: 0x%04X, Product: 0x%04X)\n",
            device->config.device_name,
            device->config.vendor_id,
            device->config.product_id);
#endif

    return device;
}

void matter_device_shutdown(MatterDevice* device) {
    if (!device) {
        return;
    }

    // Clear global device pointer if this is the global device
    if (g_matter_device == device) {
        g_matter_device = nullptr;
    }

    // Cleanup all thermostats
    for (auto& pair : device->thermostats) {
        delete pair.second;
    }
    device->thermostats.clear();

    // Shutdown Matter stack
#ifdef MATTER_SDK_AVAILABLE
    if (device->initialized) {
        Server::GetInstance().Shutdown();
        PlatformMgr().Shutdown();
        Platform::MemoryShutdown();
    }
#endif

    // Free config strings
    ::free((void*)device->config.device_name);
    ::free((void*)device->config.serial_number);

    delete device;

    fprintf(stderr, "Matter device shutdown complete\n");
}

MatterErrorCode matter_device_run_event_loop(MatterDevice* device) {
    if (!device || !device->initialized) {
        return MATTER_ERROR_NOT_INITIALIZED;
    }

    // Run Matter event loop (non-blocking)
#ifdef MATTER_SDK_AVAILABLE
    // Process pending events without blocking
    // This allows integration with Rust's async runtime
    PlatformMgr().RunEventLoop();
    // Note: For truly non-blocking operation, we could use:
    // PlatformMgr().StartEventLoopTask();
    // But that would require thread management
#endif

    return MATTER_SUCCESS;
}

bool matter_device_is_commissioned(MatterDevice* device) {
    if (!device) {
        return false;
    }

    // Check commissioning status
#ifdef MATTER_SDK_AVAILABLE
    // A device is commissioned if it has at least one fabric
    auto& fabricTable = Server::GetInstance().GetFabricTable();
    device->commissioned = (fabricTable.FabricCount() > 0);
    return device->commissioned;
#else
    return device->commissioned;
#endif
}

/* ============================================================================
 * Commissioning Functions
 * ========================================================================== */

MatterErrorCode matter_device_get_qr_code(
    MatterDevice* device,
    char* buffer,
    size_t buffer_size) {

    if (!device || !buffer || buffer_size == 0) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    // Generate QR code
#ifdef MATTER_SDK_AVAILABLE
    PayloadContents payload;
    payload.vendorID = device->config.vendor_id;
    payload.productID = device->config.product_id;
    payload.discriminator.SetLongValue(device->config.discriminator);
    payload.setUpPINCode = device->config.setup_passcode;
    payload.commissioningFlow = CommissioningFlow::kStandard;
    payload.rendezvousInformation.SetValue(RendezvousInformationFlag::kBLE);

    QRCodeSetupPayloadGenerator generator(payload);
    std::string qrCode;
    CHIP_ERROR err = generator.payloadBase38Representation(qrCode);

    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Failed to generate QR code: %s\n", ErrorStr(err));
        return MATTER_ERROR_COMMISSIONING_FAILED;
    }

    if (qrCode.length() >= buffer_size) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    strncpy(buffer, qrCode.c_str(), buffer_size - 1);
    buffer[buffer_size - 1] = '\0';

    return MATTER_SUCCESS;
#else
    // Stub: generate a placeholder QR code
    snprintf(buffer, buffer_size, "MT:Y.K%04X%04X%04X%08X",
             device->config.vendor_id,
             device->config.product_id,
             device->config.discriminator,
             device->config.setup_passcode);

    return MATTER_SUCCESS;
#endif
}

MatterErrorCode matter_device_get_pairing_code(
    MatterDevice* device,
    char* buffer,
    size_t buffer_size) {

    if (!device || !buffer || buffer_size == 0) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    // Generate manual pairing code
#ifdef MATTER_SDK_AVAILABLE
    PayloadContents payload;
    payload.vendorID = device->config.vendor_id;
    payload.productID = device->config.product_id;
    payload.discriminator.SetLongValue(device->config.discriminator);
    payload.setUpPINCode = device->config.setup_passcode;
    payload.commissioningFlow = CommissioningFlow::kStandard;

    ManualSetupPayloadGenerator generator(payload);
    std::string manualCode;
    CHIP_ERROR err = generator.payloadDecimalStringRepresentation(manualCode);

    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Failed to generate manual pairing code: %s\n", ErrorStr(err));
        return MATTER_ERROR_COMMISSIONING_FAILED;
    }

    if (manualCode.length() >= buffer_size) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    strncpy(buffer, manualCode.c_str(), buffer_size - 1);
    buffer[buffer_size - 1] = '\0';

    return MATTER_SUCCESS;
#else
    // Stub: generate a placeholder pairing code
    snprintf(buffer, buffer_size, "%011u", device->config.setup_passcode);

    return MATTER_SUCCESS;
#endif
}

void matter_device_set_commissioning_callback(
    MatterDevice* device,
    MatterCommissioningCompleteCallback callback,
    void* context) {

    if (!device) {
        return;
    }

    device->commissioning_callback = callback;
    device->commissioning_context = context;
}

/* ============================================================================
 * Thermostat Functions
 * ========================================================================== */

// Forward declaration for internal callback registration function
#ifdef MATTER_SDK_AVAILABLE
static CHIP_ERROR RegisterThermostatCallbacks(chip::EndpointId endpoint);
#else
static void RegisterThermostatCallbacks(uint16_t endpoint);
#endif

MatterThermostat* matter_thermostat_create(
    MatterDevice* device,
    const MatterThermostatConfig* config) {

    if (!device || !config) {
        return nullptr;
    }

    // Allocate thermostat structure
    MatterThermostat* thermostat = new MatterThermostat();
    if (!thermostat) {
        return nullptr;
    }

    // Initialize thermostat
    thermostat->device = device;
    thermostat->config = *config;
    thermostat->config.zone_name = config->zone_name ? ::strdup(config->zone_name) : nullptr;
    thermostat->local_temperature = 2000; // 20.0°C default
    thermostat->heating_setpoint = 2000;
    thermostat->cooling_setpoint = 2400;
    thermostat->system_mode = MATTER_THERMOSTAT_MODE_HEAT;
    thermostat->running_state = MATTER_THERMOSTAT_RUNNING_IDLE;
    thermostat->setpoint_callback = nullptr;
    thermostat->setpoint_context = nullptr;
    thermostat->mode_callback = nullptr;
    thermostat->mode_context = nullptr;

    // Register thermostat endpoint
#ifdef MATTER_SDK_AVAILABLE
    // Enable the endpoint
    EmberAfStatus status = emberAfEndpointEnableDisable(config->endpoint_id, true);
    if (status != EMBER_ZCL_STATUS_SUCCESS) {
        fprintf(stderr, "Failed to enable endpoint %u: status 0x%02X\n",
                config->endpoint_id, status);
        // Continue anyway - endpoint may already be enabled
    }

    // Initialize thermostat cluster attributes
    // Note: Actual attribute registration is typically done via ZAP configuration
    // and code generation, but we can set initial values here

    // The thermostat cluster and attributes are defined in the ZAP configuration
    // and generated code. This wrapper just manages the state and callbacks.

    // Register attribute change callbacks
    CHIP_ERROR err = RegisterThermostatCallbacks(config->endpoint_id);
    if (err != CHIP_NO_ERROR) {
        fprintf(stderr, "Warning: Failed to register callbacks for endpoint %u: %s\n",
                config->endpoint_id, ErrorStr(err));
        // Continue anyway - callbacks may still work
    }
#else
    // Stub: Register callbacks (no actual Matter stack to register with)
    RegisterThermostatCallbacks(config->endpoint_id);
#endif

    device->thermostats[config->endpoint_id] = thermostat;

    fprintf(stderr, "Matter thermostat created: %s (Endpoint: %u)\n",
            thermostat->config.zone_name,
            thermostat->config.endpoint_id);

    return thermostat;
}

void matter_thermostat_destroy(MatterThermostat* thermostat) {
    if (!thermostat) {
        return;
    }

    // Remove from device
    if (thermostat->device) {
        thermostat->device->thermostats.erase(thermostat->config.endpoint_id);
    }

    // Free config strings
    ::free((void*)thermostat->config.zone_name);

    delete thermostat;
}

MatterErrorCode matter_thermostat_set_local_temperature(
    MatterThermostat* thermostat,
    int16_t temperature) {

    if (!thermostat) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    thermostat->local_temperature = temperature;

    // Update Matter attribute
#ifdef MATTER_SDK_AVAILABLE
    EmberAfStatus status = emberAfWriteServerAttribute(
        thermostat->config.endpoint_id,
        Thermostat::Id,
        Thermostat::Attributes::LocalTemperature::Id,
        reinterpret_cast<uint8_t*>(&temperature),
        ZCL_INT16S_ATTRIBUTE_TYPE
    );

    if (status != EMBER_ZCL_STATUS_SUCCESS) {
        fprintf(stderr, "Failed to write LocalTemperature attribute: status 0x%02X\n", status);
        return MATTER_ERROR_CLUSTER_FAILED;
    }
#endif

    return MATTER_SUCCESS;
}

MatterErrorCode matter_thermostat_get_heating_setpoint(
    MatterThermostat* thermostat,
    int16_t* setpoint) {

    if (!thermostat || !setpoint) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    *setpoint = thermostat->heating_setpoint;
    return MATTER_SUCCESS;
}

MatterErrorCode matter_thermostat_get_cooling_setpoint(
    MatterThermostat* thermostat,
    int16_t* setpoint) {

    if (!thermostat || !setpoint) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    *setpoint = thermostat->cooling_setpoint;
    return MATTER_SUCCESS;
}

MatterErrorCode matter_thermostat_get_system_mode(
    MatterThermostat* thermostat,
    MatterThermostatMode* mode) {

    if (!thermostat || !mode) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    *mode = thermostat->system_mode;
    return MATTER_SUCCESS;
}

MatterErrorCode matter_thermostat_set_running_state(
    MatterThermostat* thermostat,
    uint16_t state) {

    if (!thermostat) {
        return MATTER_ERROR_INVALID_PARAM;
    }

    thermostat->running_state = state;

    // Update Matter attribute
#ifdef MATTER_SDK_AVAILABLE
    EmberAfStatus status = emberAfWriteServerAttribute(
        thermostat->config.endpoint_id,
        Thermostat::Id,
        Thermostat::Attributes::ThermostatRunningState::Id,
        reinterpret_cast<uint8_t*>(&state),
        ZCL_BITMAP16_ATTRIBUTE_TYPE
    );

    if (status != EMBER_ZCL_STATUS_SUCCESS) {
        fprintf(stderr, "Failed to write ThermostatRunningState attribute: status 0x%02X\n", status);
        return MATTER_ERROR_CLUSTER_FAILED;
    }
#endif

    return MATTER_SUCCESS;
}

void matter_thermostat_set_setpoint_callback(
    MatterThermostat* thermostat,
    MatterThermostatSetpointChangedCallback callback,
    void* context) {

    if (!thermostat) {
        return;
    }

    thermostat->setpoint_callback = callback;
    thermostat->setpoint_context = context;
}

void matter_thermostat_set_mode_callback(
    MatterThermostat* thermostat,
    MatterThermostatModeChangedCallback callback,
    void* context) {

    if (!thermostat) {
        return;
    }

    thermostat->mode_callback = callback;
    thermostat->mode_context = context;
}

/* ============================================================================
 * Callback handlers (called by Matter stack)
 * ========================================================================== */

#ifdef MATTER_SDK_AVAILABLE
/**
 * Matter attribute write callback
 *
 * Called by the Matter stack when a controller writes to a thermostat attribute
 */
void MatterThermostatAttributeWriteCallback(
    chip::EndpointId endpoint,
    chip::AttributeId attributeId,
    uint8_t* value,
    uint16_t size) {

    if (!g_matter_device || !value) {
        return;
    }

    // Find thermostat by endpoint
    auto it = g_matter_device->thermostats.find(endpoint);
    if (it == g_matter_device->thermostats.end()) {
        fprintf(stderr, "Thermostat not found for endpoint %u\n", endpoint);
        return;
    }

    MatterThermostat* thermostat = it->second;

    // Handle different attribute types
    if (attributeId == Thermostat::Attributes::OccupiedHeatingSetpoint::Id) {
        // Heating setpoint changed
        if (size >= sizeof(int16_t)) {
            // Use memcpy to avoid unaligned memory access
            int16_t setpoint;
            std::memcpy(&setpoint, value, sizeof(int16_t));
            thermostat->heating_setpoint = setpoint;

            fprintf(stderr, "Endpoint %u: Heating setpoint changed to %d centi-°C\n",
                    endpoint, setpoint);

            if (thermostat->setpoint_callback) {
                thermostat->setpoint_callback(
                    thermostat->setpoint_context,
                    endpoint,
                    thermostat->heating_setpoint,
                    thermostat->cooling_setpoint
                );
            }
        }
    } else if (attributeId == Thermostat::Attributes::OccupiedCoolingSetpoint::Id) {
        // Cooling setpoint changed
        if (size >= sizeof(int16_t)) {
            // Use memcpy to avoid unaligned memory access
            int16_t setpoint;
            std::memcpy(&setpoint, value, sizeof(int16_t));
            thermostat->cooling_setpoint = setpoint;

            fprintf(stderr, "Endpoint %u: Cooling setpoint changed to %d centi-°C\n",
                    endpoint, setpoint);

            if (thermostat->setpoint_callback) {
                thermostat->setpoint_callback(
                    thermostat->setpoint_context,
                    endpoint,
                    thermostat->heating_setpoint,
                    thermostat->cooling_setpoint
                );
            }
        }
    } else if (attributeId == Thermostat::Attributes::SystemMode::Id) {
        // System mode changed
        if (size >= sizeof(uint8_t)) {
            // Single byte, no alignment issue, but consistent with above
            uint8_t mode = *value;
            thermostat->system_mode = static_cast<MatterThermostatMode>(mode);

            fprintf(stderr, "Endpoint %u: System mode changed to 0x%02X\n",
                    endpoint, mode);

            if (thermostat->mode_callback) {
                thermostat->mode_callback(
                    thermostat->mode_context,
                    endpoint,
                    thermostat->system_mode
                );
            }
        }
    }
}

/**
 * Matter framework callback - called automatically after attribute changes
 *
 * This is a well-known callback hook that the Matter/Ember framework calls
 * after any attribute is written. We use it to dispatch to our thermostat
 * callback handler.
 *
 * This function is automatically invoked by the Matter stack - no explicit
 * registration is needed.
 */
void emberAfPostAttributeChangeCallback(chip::EndpointId endpoint,
                                        chip::ClusterId clusterId,
                                        chip::AttributeId attributeId,
                                        uint8_t mask,
                                        uint16_t manufacturerCode,
                                        uint8_t type,
                                        uint16_t size,
                                        uint8_t* value) {
    // Only handle thermostat cluster attributes
    if (clusterId == Thermostat::Id) {
        MatterThermostatAttributeWriteCallback(endpoint, attributeId, value, size);
    }
}

/**
 * Register attribute change callbacks for a thermostat endpoint
 *
 * Note: The actual callback registration happens via emberAfPostAttributeChangeCallback
 * which is automatically called by the Matter framework. This function just performs
 * endpoint validation and logging.
 *
 * This function is internal-only and not exposed to Rust via FFI.
 */
static CHIP_ERROR RegisterThermostatCallbacks(chip::EndpointId endpoint) {
    fprintf(stderr, "Matter callbacks registered for thermostat endpoint %u\n", endpoint);
    fprintf(stderr, "  - Callbacks will be invoked via emberAfPostAttributeChangeCallback\n");
    return CHIP_NO_ERROR;
}
#else
/**
 * Stub implementation when Matter SDK is not available
 *
 * This function is internal-only and not exposed to Rust via FFI.
 */
static void RegisterThermostatCallbacks(uint16_t endpoint) {
    fprintf(stderr, "Matter callbacks registered (STUB) for thermostat endpoint %u\n", endpoint);
}
#endif

/* ============================================================================
 * Utility Functions
 * ========================================================================== */

const char* matter_get_error_message(MatterErrorCode error) {
    if (error < 0 || error > MATTER_ERROR_UNKNOWN) {
        return "Invalid error code";
    }
    return error_messages[error];
}

const char* matter_get_version(void) {
#ifdef MATTER_SDK_AVAILABLE
    // Return the actual Matter SDK version
    return "Matter SDK - Full Implementation";
#else
    return "1.0.0-stub";
#endif
}
