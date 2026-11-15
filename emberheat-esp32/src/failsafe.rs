//! Crash fail-safe: relays OFF and a restart after any panic or hang
//!
//! A crashed controller must never leave a heating relay energised. Two
//! independent mechanisms make sure of that:
//!
//! - **Panic path.** `esp-backtrace` calls [`custom_pre_backtrace`] before it
//!   prints anything and [`custom_halt`] when it is done. The first drives
//!   every relay pin HIGH (OFF) with a single register write; the second
//!   restarts the chip. CPU faults (stack overflow, illegal access) are turned
//!   into panics by esp-hal, so they take the same path.
//! - **Watchdog.** The RTC watchdog (RWDT) resets the whole chip unless
//!   [`watchdog_task`] keeps feeding it, and that task only feeds while every
//!   zone control loop is still reporting in through [`zone_beat`]. RWDT runs
//!   in the low-power domain on its own clock, so it still fires when the
//!   executor, the interrupts or the main clock are stuck.
//!
//! Nothing here uses the allocator, the async executor or a lock in the panic
//! path: by the time it runs, any of those may be the thing that broke.
//!
//! After a reset the relay pins are not driven until `main` configures them
//! again, so "OFF during reset" relies on the relay inputs being pulled HIGH
//! by the hardware.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::{Duration, Instant, Timer};
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};

use crate::config::{
    NUM_ZONES, PANIC_RESTART_DELAY_MS, WATCHDOG_TIMEOUT_SECS, ZONE_CONFIGS, ZONE_STALL_TIMEOUT_SECS,
};

/// How often the supervisor checks the zone heartbeats and feeds the watchdog
const SUPERVISOR_INTERVAL_SECS: u64 = 2;

// The watchdog must outlast several supervisor rounds, or ordinary jitter
// (flash erases, commissioning crypto) would reset a healthy controller.
const _: () = assert!(
    WATCHDOG_TIMEOUT_SECS >= 4 * SUPERVISOR_INTERVAL_SECS,
    "WATCHDOG_TIMEOUT_SECS is too short for the supervisor interval"
);

/// Bit mask of every configured relay pin in the GPIO output register
pub const RELAY_PIN_MASK: u32 = {
    let mut mask = 0u32;
    let mut i = 0;
    while i < NUM_ZONES {
        assert!(
            ZONE_CONFIGS[i].relay_pin < 32,
            "relay pin does not fit the GPIO output register"
        );
        mask |= 1 << ZONE_CONFIGS[i].relay_pin;
        i += 1;
    }
    mask
};

/// One counter per zone, bumped by its control loop on every iteration
static ZONE_BEATS: [AtomicU32; NUM_ZONES] = [const { AtomicU32::new(0) }; NUM_ZONES];

/// Drive every relay pin HIGH (relay OFF, active-low) with one register write
///
/// Safe to call from any context, including a panic with the executor dead:
/// `OUT_W1TS` only sets bits, so there is no read-modify-write to race with.
/// Pins that are not configured as outputs yet are unaffected (they are still
/// in their reset state, which the hardware pulls to OFF).
pub fn force_relays_off() {
    esp_hal::peripherals::GPIO::regs()
        .out_w1ts()
        .write(|w| unsafe { w.bits(RELAY_PIN_MASK) });
}

/// Record that a zone's control loop completed another iteration
///
/// Called from the top of each loop, right after its ticker fires, so a loop
/// that is stuck anywhere in its body stops reporting in.
pub fn zone_beat(zone_index: usize) {
    if let Some(beat) = ZONE_BEATS.get(zone_index) {
        beat.fetch_add(1, Ordering::Relaxed);
    }
}

/// Arm the hardware watchdog and keep it fed while every zone loop is alive
///
/// Spawn this together with the zone tasks. If a zone stops reporting in for
/// `ZONE_STALL_TIMEOUT_SECS` this panics, which turns the relays OFF and
/// restarts through the panic path. If this task itself stops running (the
/// executor is stuck), RWDT resets the chip `WATCHDOG_TIMEOUT_SECS` after the
/// last feed.
#[embassy_executor::task]
pub async fn watchdog_task(mut rtc: Rtc<'static>) -> ! {
    rtc.rwdt.set_timeout(
        RwdtStage::Stage0,
        esp_hal::time::Duration::from_secs(WATCHDOG_TIMEOUT_SECS),
    );
    rtc.rwdt.enable();
    // `enable` already selects a full reset for stage 0; say so explicitly so
    // the safety behaviour does not depend on a driver default.
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetSystem);
    rtc.rwdt.feed();
    log::info!(
        "Watchdog armed: reset after {}s without a feed, zone stall limit {}s",
        WATCHDOG_TIMEOUT_SECS,
        ZONE_STALL_TIMEOUT_SECS
    );

    let stall_limit = Duration::from_secs(ZONE_STALL_TIMEOUT_SECS);
    let mut last_count = [0u32; NUM_ZONES];
    let mut last_change = [Instant::now(); NUM_ZONES];

    loop {
        Timer::after(Duration::from_secs(SUPERVISOR_INTERVAL_SECS)).await;

        let now = Instant::now();
        for i in 0..NUM_ZONES {
            let count = ZONE_BEATS[i].load(Ordering::Relaxed);
            if count != last_count[i] {
                last_count[i] = count;
                last_change[i] = now;
            } else if now.duration_since(last_change[i]) > stall_limit {
                panic!(
                    "{} control loop has not run for {}s",
                    ZONE_CONFIGS[i].name, ZONE_STALL_TIMEOUT_SECS
                );
            }
        }

        rtc.rwdt.feed();
    }
}

/// Called by `esp-backtrace` first thing in a panic, before any printing
///
/// Interrupts are turned off for good so nothing else (another esp-rtos
/// thread, an interrupt handler) can run and switch a relay back on while the
/// backtrace prints and the restart delay runs.
#[no_mangle]
pub fn custom_pre_backtrace() {
    // Clear mstatus.MIE: global interrupt disable on RISC-V.
    unsafe { core::arch::asm!("csrci mstatus, 8") };
    force_relays_off();
}

/// Called by `esp-backtrace` after the panic message and backtrace are printed
///
/// Waits a little so a panic that repeats at boot cannot restart the chip in a
/// tight loop (and the message stays readable on serial), then resets.
#[no_mangle]
pub fn custom_halt() -> ! {
    force_relays_off();
    esp_println::println!(
        "Relays forced OFF. Restarting in {} ms...",
        PANIC_RESTART_DELAY_MS
    );
    esp_hal::delay::Delay::new().delay_millis(PANIC_RESTART_DELAY_MS);
    force_relays_off();
    esp_hal::system::software_reset()
}
