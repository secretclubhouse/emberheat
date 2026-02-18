# MQTT and Matter Integration

This document explains how MQTT telemetry works alongside Matter smart home control in emberheat-esp32, and the architectural decisions made to solve concurrency issues.

## Overview

The thermostat supports two networking modes:

1. **MQTT-only mode** (default): WiFi connects directly, MQTT publishes temperature telemetry
2. **Matter mode** (`--features matter`): Matter handles WiFi/BLE, MQTT runs alongside for telemetry

In Matter mode, both protocols share the same WiFi connection managed by rs-matter-embassy.

## The Problem: RefCell Already Borrowed

### Initial Approach (Failed)

The first implementation attempted to run MQTT as a separate Embassy task:

```rust
// main.rs - BROKEN APPROACH
#[embassy_executor::task]
async fn mqtt_matter_task(config: MqttConfig) {
    // Wait for Matter to signal that Stack is ready
    let signal = STACK_READY.wait().await;
    let stack = get_stack();
    
    // Run MQTT with the extracted stack
    mqtt::mqtt_task(stack, config, &MQTT_CHANNEL).await
}

// Spawned separately from Matter task
spawner.spawn(matter_main_task(...));
spawner.spawn(mqtt_matter_task(mqtt_config));
```

### The Panic

This caused a runtime panic:

```
panicked at embassy-net-0.7.1/src/lib.rs:394:27:
RefCell already borrowed
```

### Root Cause

The `embassy_net::Stack` uses `RefCell` for interior mutability:

```rust
// Inside embassy-net Stack
pub struct Stack<'d> {
    inner: &'d RefCell<StackInner<'d>>,
    // ...
}
```

Even on a single-threaded Embassy executor, `RefCell` borrows can conflict across `.await` points:

```
Timeline:
─────────────────────────────────────────────────────────────────────
Matter task:    [borrow Stack]──────[.await]──────[still borrowed]
                                        │
MQTT task:      ────────────────────────┼────[try borrow]──> PANIC!
                                        │
                                   executor switches
                                   to MQTT task
─────────────────────────────────────────────────────────────────────
```

When Matter's networking code borrows the Stack and hits an `.await`, the executor may switch to the MQTT task. If MQTT then tries to borrow the same Stack, the `RefCell` panics because it's already borrowed.

### Why This Happens

1. **Embassy is single-threaded** - Tasks don't run in parallel, but they do interleave at await points
2. **RefCell isn't aware of async** - It sees two active borrows regardless of which task holds them
3. **Stack borrows span awaits** - Network operations naturally hold borrows across async boundaries

## The Solution: Run MQTT Inside UserTask

Instead of running MQTT as a separate task, run it inside Matter's `UserTask`. The rs-matter-embassy stack uses `select3` internally to run multiple futures together, which properly interleaves borrows.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Matter Stack (run_coex)                       │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │              │  │              │  │                      │  │
│  │ WiFi/BLE     │  │ Matter       │  │ UserTask             │  │
│  │ Driver       │  │ Protocol     │  │ (MqttBridge)         │  │
│  │              │  │              │  │                      │  │
│  │              │  │              │  │  - Extract Stack     │  │
│  │              │  │              │  │  - Run MQTT client   │  │
│  │              │  │              │  │  - Publish messages  │  │
│  │              │  │              │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│         │                 │                    │                 │
│         └─────────────────┼────────────────────┘                │
│                           │                                      │
│                     select3(...)                                 │
│              (proper borrow interleaving)                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation

The `MqttBridge` struct implements the `UserTask` trait:

```rust
// matter/mqtt_bridge.rs
pub struct MqttBridge {
    config: MqttConfig,
    channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
}

impl UserTask for MqttBridge {
    fn run<S, N>(&mut self, _net_stack: S, netif: N) -> impl Future<Output = Result<(), Error>>
    where
        S: NetStack,
        N: NetifDiag + NetChangeNotif,
    {
        let config = self.config.clone();
        let channel = self.channel;

        async move {
            // Extract Stack from EnetNetif (our fork makes this field public)
            let stack: Stack<'static> = unsafe {
                let netif_ptr = &netif as *const N as *const EnetNetif<'static>;
                (*netif_ptr).stack
            };

            // Run MQTT - borrows interleave correctly with Matter via select3
            mqtt_task_inner(stack, config, channel).await;
            Ok(())
        }
    }
}
```

The bridge is passed to `run_coex()`:

```rust
// matter/stack.rs
pub async fn matter_task(
    controller: &'static EspRadioController<'static>,
    wifi: WIFI<'static>,
    bt: BT<'static>,
    mqtt_bridge: MqttBridge,  // Runs MQTT inside Matter's task context
) {
    // ...
    let matter = pin!(stack.run_coex(
        EmbassyWifi::new(EspWifiDriver::new(controller, wifi, bt), stack),
        &persist,
        (NODE, handler),
        mqtt_bridge,  // UserTask runs alongside Matter
    ));
    matter.await.unwrap();
}
```

### Why This Works

1. **Single logical task** - MQTT runs inside Matter's task via `select3`, not as a separate spawned task
2. **Cooperative interleaving** - `select3` polls futures one at a time, ensuring borrows don't overlap
3. **No RefCell conflicts** - Only one future holds a Stack borrow at any point in the `select3` loop

### Fork Requirement

This solution requires access to the `embassy_net::Stack` from within the `UserTask`. The rs-matter-embassy library encapsulates this in `EnetNetif`, so we use a fork that makes the field public:

```toml
# Cargo.toml
rs-matter-embassy = { 
    git = "https://github.com/secretclubhouse/rs-matter-embassy", 
    branch = "stack-public"
}
```

The fork change is minimal - just making `EnetNetif.stack` public.

### Memory Requirements

Running MQTT inside the UserTask requires additional bump allocator memory. The rs-matter-stack uses a bump allocator for async futures, and the full Matter+MQTT stack has significant memory requirements:

| Component | Memory |
|-----------|--------|
| Base WiFi + BLE drivers | ~16.5 KB |
| BLE commissioning mode | ~6 KB |
| Matter responder handlers (4x) | ~4.5 KB |
| MQTT TCP socket buffers | 8 KB (4KB rx + 4KB tx) |
| MQTT client buffers | 2 KB (1KB recv + 1KB write) |
| Headroom | ~3 KB |
| **Total required** | **~40 KB** |

The `BUMP_SIZE` constant in `matter/stack.rs` must be set to accommodate this:

```rust
// matter/stack.rs
const BUMP_SIZE: usize = 40000;  // 40KB for Matter + BLE + MQTT
```

**Symptom of insufficient memory:**

```
panicked at rs-matter-stack/.../bump.rs:212:9:
Out of bump memory
```

The bump allocator logs show usage before the panic. For example, with insufficient memory:

```
BUMP[.../lib.rs:753]: 1112b (U:27188b/F:812b)  // Only 812 bytes free
                                                // Next 1112b handler alloc fails
```

If you see this panic, increase `BUMP_SIZE`. The ESP32-C6 has 512KB of RAM, so there's headroom for larger values if needed. Monitor the BUMP logs during startup to see actual usage.

## Message Flow

Temperature readings flow from zone control loops to MQTT via a channel:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Zone Control   │     │  MQTT Channel   │     │  MqttBridge     │
│  Loop (zone1)   │────>│                 │────>│  (UserTask)     │
│                 │     │  static Channel │     │                 │
│  Zone Control   │────>│  <MqttMessage>  │     │  Publishes to   │
│  Loop (zone2)   │     │                 │     │  MQTT broker    │
│       ...       │     │                 │     │                 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

The channel is bounded (16 messages) so zone loops never block for long, and MQTT publishes messages as fast as the network allows.

## Alternative Approaches Considered

### 1. Mutex Instead of RefCell

Could embassy-net use a Mutex instead? No - `RefCell` is intentional for single-threaded performance. A Mutex would add overhead and still wouldn't solve the async borrow problem without being async-aware.

### 2. Clone the Stack

`Stack` is `Copy`, so we tried storing a copy in a static. But the copy still references the same internal `RefCell`, so the problem persists.

### 3. Separate Network Interface

Could create a second TCP socket pool? No - embassy-net manages sockets through a single Stack instance per network interface.

### 4. PreexistingWireless Pattern

rs-matter-embassy supports passing a pre-initialized WiFi connection. This would give us direct Stack access but requires replicating complex WiFi/BLE coexistence initialization - too much complexity for this use case.

## Testing

The integration can be verified by:

1. Build: `make build-matter`
2. Flash: `make flash-matter`
3. Monitor serial output for:
   - "Matter: Running..."
   - "MQTT bridge: Stack extracted, starting MQTT task"
   - "MQTT connected successfully"
4. Check MQTT broker receives temperature messages at `thermostat/{device_id}/temp/{zone}`

## References

- [Embassy async executor](https://embassy.dev/book/dev/runtime.html)
- [RefCell documentation](https://doc.rust-lang.org/std/cell/struct.RefCell.html)
- [rs-matter-embassy UserTask](https://github.com/sysgrok/rs-matter-embassy)
