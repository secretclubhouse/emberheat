# Crash fail-safe — design and bench procedure

Date: 2026-11-14
Status: approved by Josh 2026-11-14; implemented on `fix/crash-fail-safe`, not yet bench-tested

## Intent

A firmware panic used to print a backtrace and halt forever with the GPIO pins
frozen. A relay that was ON stayed ON until someone power-cycled the controller.
Nothing could restart the chip either: esp-hal turns every watchdog off at
startup and nothing re-enabled one. The firmware now parses network input and can
run relay ON phases unattended (duty-cycle fallback), so a crash must fail safe.

## Goals

- After any panic, CPU fault or hang, every relay ends up OFF (pin HIGH) and the
  controller restarts.
- The panic path needs no executor, allocator or lock.
- The watchdog proves the zone control loops are alive, not just "some task".

## Non-goals

- Detecting a loop that keeps running but holds a relay ON through a logic bug.
- Alerting (MQTT/Matter) that a restart happened. The reset reason is logged on
  serial only.
- A boot-failure counter or safe mode after repeated restarts.

## Approaches considered

1. **Panic hooks + RTC watchdog fed by a zone-heartbeat supervisor (chosen).**
   Covers panics, faults, a hung zone loop and a dead executor, with a hardware
   timer that does not share a clock with the thing it guards.
2. Timer-group watchdog (MWDT) instead of RWDT. Rejected: it runs from the main
   clock domain, which is part of what may be stuck.
3. Feed the watchdog from the `main` idle loop. Rejected: proves the executor is
   alive, not that any zone loop is.
4. Relays OFF then halt, no restart. Rejected: no heat until a power cycle.

## Design

All in `emberheat-esp32/src/failsafe.rs` unless noted.

**Panic path.** `esp-backtrace` is built with `custom-pre-backtrace` and
`custom-halt`.

- `custom_pre_backtrace` runs before anything is printed. It disables interrupts
  for good (so no other thread or handler can switch a relay back on) and calls
  `force_relays_off()`: one write to the GPIO `OUT_W1TS` register with
  `RELAY_PIN_MASK`, a compile-time constant built from `ZONE_CONFIGS`.
- `custom_halt` runs after the backtrace. It forces the relays OFF again,
  busy-waits `PANIC_RESTART_DELAY_MS` (5 s) and calls
  `esp_hal::system::software_reset()`.
- esp-hal turns CPU exceptions (stack overflow, illegal access) into panics, so
  they take the same path.

**Watchdog.**

- Each zone control loop calls `failsafe::zone_beat(index)` at the top of every
  iteration, right after its 6 s ticker fires.
- `watchdog_task` arms the RTC watchdog (RWDT, stage 0 = full system reset,
  `WATCHDOG_TIMEOUT_SECS` = 20 s) and wakes every 2 s. If every zone has beaten
  within `ZONE_STALL_TIMEOUT_SECS` (20 s) it feeds the watchdog. If one has not,
  it panics, which takes the panic path above.
- If the supervisor itself stops running, RWDT resets the chip 20 s after the
  last feed.
- The task is spawned right after the zone tasks. Before that the relays are OFF
  and network bring-up may legitimately wait forever (MQTT mode waits for WiFi),
  so the watchdog is not armed during boot.

**Boot order (`main.rs`).** The relay pins are created `Level::High` immediately
after `esp_hal::init`, before the heap, scheduler or any network work. The reset
reason is logged at boot.

**Pin mapping (`main.rs`).** `main` hardcodes which `peripherals.GPIOn` each zone
drives. A compile-time assert checks that against `ZONE_CONFIGS[n].relay_pin`, so
the panic mask cannot drift from the pins actually driven.

## Trade-offs

- **Restart loop.** A panic that repeats at boot restarts the chip every
  5–10 s indefinitely. Relays stay OFF throughout: the failure is "no heat", not
  "stuck heat". Nothing reports it except the serial log.
- **Matter commissioning and flash writes.** Flash erases and commissioning
  crypto block the executor for milliseconds to a few seconds, well inside the
  20 s limits. A reset during commissioning fails that attempt. Zone state is
  written through `sequential-storage`, which tolerates interrupted writes.
  This margin is reasoned from the code, not measured: bench step 5 checks it.
- **Worst-case relay ON time.** Panic or fault: milliseconds. One zone loop hung:
  about 22 s (20 s stall limit + 2 s supervisor round), then the panic path.
  Executor dead: up to 20 s after the last feed.
- **During reset.** Pins are not driven from the reset until `main` reconfigures
  them (well under a second). OFF in that window depends on the relay inputs
  being pulled HIGH by the hardware. Bench step 1 checks it.
- **After restart.** Setpoints and modes come back from flash. Duty-cycle
  fallback history is RAM only, so fallback starts in its off phase.

## Bench procedure

Use the dual-radio rig (`make flash-monitor-matter-dual-radio` or the rig's usual
flash target) with a relay, LED or meter on the Zone1 relay pin (GPIO0). The
temporary code below goes in the Matter `zone_control_loop` in `main.rs`, right
after the `failsafe::zone_beat` call. Do not commit it.

1. **Reset state.** Hold the C6 in reset (EN low). The relay pin must read HIGH
   and the relay must be OFF. If it floats or the relay closes, the board needs a
   pull-up on that line before this design is safe.
2. **Deliberate panic with relay ON.** Add:

   ```rust
   if state.heating_active && Instant::now().as_secs() > 60 {
       panic!("bench: deliberate panic");
   }
   ```

   Flash, raise the setpoint so the relay turns ON, wait for the 60 s mark.
   Expect: relay OFF at once, the panic message and backtrace, "Relays forced
   OFF. Restarting in 5000 ms...", a restart about 5 s later, and a software
   reset reason in the boot log. The relay must stay OFF through the restart
   until the control loop turns it on again.
3. **Hung zone loop with relay ON.** Replace the panic with:

   ```rust
   if state.heating_active && Instant::now().as_secs() > 60 {
       core::future::pending::<()>().await;
   }
   ```

   Expect: about 20–22 s after the hang, a panic "Zone1 control loop has not run
   for 20s", relay OFF, restart as in step 2.
4. **Dead executor with relay ON.** Replace the body with `loop {}`.
   Expect: no output, then a reset within 20 s. The relay goes OFF at the reset
   and the boot log shows a watchdog reset reason (not software reset).
5. **No false resets.** Remove the temporary code and flash the real firmware.
   Factory-reset, commission with Apple Home, change setpoints a few times, and
   leave it running for an hour. The "Reset reason" line must appear only once
   (the power-up or flash reset).

## Left to later

- Publish the reset reason over MQTT so a restart loop is visible without serial.
- Boot-failure counter with a back-off or safe mode.
