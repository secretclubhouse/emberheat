//! Duty tracking and the bounded fallback relay cycle.
//!
//! When a zone's remote temperature readings stop (Wi-Fi or broker outage,
//! dead sensor battery), turning the relay off leaves the room to go cold.
//! Josh, 2026-11-14: a slightly warm room beats a cold one after an overnight
//! outage. So the zone keeps heating at its recent duty cycle instead, but only
//! for a bounded time (`FallbackConfig::max_secs`), with the duty capped, and
//! never without history to base it on. Pure logic with no allocation.

/// Tuning for the fallback cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallbackConfig {
    /// Longest time to keep cycling after readings stop, in seconds.
    pub max_secs: u64,
    /// Length of one on/off cycle, in seconds.
    pub period_secs: u64,
    /// Upper bound on the duty fraction used, so a long heating run before an
    /// outage cannot turn into continuous heating.
    pub max_duty: f32,
    /// On-times shorter than this are skipped, to avoid pointless relay wear.
    pub min_on_secs: u64,
}

impl FallbackConfig {
    /// 6 h limit, 30 min cycle, at most 75 % duty, at least 3 min on.
    pub const DEFAULT: Self = Self {
        max_secs: 21_600,
        period_secs: 1_800,
        max_duty: 0.75,
        min_on_secs: 180,
    };
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Time constant of the duty average, in seconds. Long enough to smooth over
/// individual hysteresis cycles, short enough to follow the weather.
pub const DUTY_TIME_CONSTANT_SECS: f32 = 7200.0;

/// Exponentially weighted average of how much of the time the relay was on.
#[derive(Debug, Clone, Copy)]
pub struct DutyTracker {
    duty: f32,
}

impl DutyTracker {
    /// Starts at zero duty: with no history, fallback never heats.
    pub const fn new() -> Self {
        Self { duty: 0.0 }
    }

    /// Folds in `dt_secs` seconds during which the relay was `relay_on`.
    pub fn update(&mut self, relay_on: bool, dt_secs: u32) {
        let alpha = (dt_secs as f32 / DUTY_TIME_CONSTANT_SECS).min(1.0);
        let x = if relay_on { 1.0 } else { 0.0 };
        self.duty += alpha * (x - self.duty);
    }

    /// Current duty fraction, `0.0..=1.0`.
    pub fn duty(&self) -> f32 {
        self.duty
    }
}

impl Default for DutyTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Relay state for `elapsed_secs` into an outage, or `None` once the outage
/// has outlasted `cfg.max_secs`.
pub fn fallback_relay(duty: f32, elapsed_secs: u64, cfg: &FallbackConfig) -> Option<bool> {
    if elapsed_secs > cfg.max_secs {
        return None;
    }
    let d = if duty.is_finite() {
        duty.clamp(0.0, cfg.max_duty)
    } else {
        0.0
    };
    let mut on_secs = (d * cfg.period_secs as f32) as u64;
    if on_secs < cfg.min_on_secs {
        on_secs = 0;
    }
    // `period_secs` is non-zero in any sane config; guard so a zero cannot panic.
    let phase = elapsed_secs.checked_rem(cfg.period_secs).unwrap_or(0);
    Some(phase < on_secs)
}

/// What to do with the relay when a pass has no usable reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackDecision {
    /// Drive the relay to this state.
    Relay(bool),
    /// The outage outlasted the limit; leave the relay off.
    Expired,
    /// No reading was ever seen since boot; leave the relay off.
    NoHistory,
}

/// Health of a zone's temperature input, for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorStatus {
    /// Fresh readings are arriving.
    Ok,
    /// Readings stopped; the fallback cycle is driving the relay.
    Fallback,
    /// Readings stopped and fallback is not (or no longer) driving the relay.
    Lost,
}

impl SensorStatus {
    /// Lowercase name used in published status messages.
    pub const fn as_str(self) -> &'static str {
        match self {
            SensorStatus::Ok => "ok",
            SensorStatus::Fallback => "fallback",
            SensorStatus::Lost => "lost",
        }
    }
}

/// Per-zone fallback state: duty history plus when the current outage began.
#[derive(Debug, Clone, Copy)]
pub struct FallbackController {
    tracker: DutyTracker,
    lost_at_secs: Option<u64>,
    has_history: bool,
}

impl FallbackController {
    pub const fn new() -> Self {
        Self {
            tracker: DutyTracker::new(),
            lost_at_secs: None,
            has_history: false,
        }
    }

    /// Call on every pass that had a good reading, after the relay was set.
    /// Ends any outage, so the next one restarts the clock.
    pub fn on_reading(&mut self, relay_on: bool, dt_secs: u32) {
        self.lost_at_secs = None;
        self.has_history = true;
        self.tracker.update(relay_on, dt_secs);
    }

    /// Call on every pass whose reading was `SensorError::Unavailable`.
    /// The duty is frozen during the outage, so fallback heating cannot feed
    /// back into the history it is based on.
    pub fn on_unavailable(&mut self, now_secs: u64, cfg: &FallbackConfig) -> FallbackDecision {
        if !self.has_history {
            return FallbackDecision::NoHistory;
        }
        let lost_at = *self.lost_at_secs.get_or_insert(now_secs);
        match fallback_relay(self.tracker.duty(), now_secs.saturating_sub(lost_at), cfg) {
            Some(on) => FallbackDecision::Relay(on),
            None => FallbackDecision::Expired,
        }
    }

    pub fn duty(&self) -> f32 {
        self.tracker.duty()
    }
}

impl Default for FallbackController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const C: FallbackConfig = FallbackConfig::DEFAULT;

    #[test]
    fn tracker_starts_at_zero_and_converges() {
        let mut t = DutyTracker::new();
        assert_eq!(t.duty(), 0.0);
        for _ in 0..1200 {
            t.update(true, 6);
        } // 2 h on
        assert!((t.duty() - 0.632).abs() < 0.01);
        for _ in 0..6000 {
            t.update(true, 6);
        } // +10 h on
        assert!(t.duty() > 0.99 && t.duty() <= 1.0);
        for _ in 0..7200 {
            t.update(false, 6);
        }
        assert!(t.duty() < 0.01);
    }
    #[test]
    fn tracker_follows_a_half_duty_pattern() {
        let mut t = DutyTracker::new();
        for i in 0..12_000 {
            t.update((i / 100) % 2 == 0, 6);
        } // 10 min on, 10 min off, 20 h
        assert!((t.duty() - 0.5).abs() < 0.05);
    }
    #[test]
    fn cycle_edges() {
        assert_eq!(fallback_relay(0.5, 0, &C), Some(true));
        assert_eq!(fallback_relay(0.5, 899, &C), Some(true));
        assert_eq!(fallback_relay(0.5, 900, &C), Some(false));
        assert_eq!(fallback_relay(0.5, 1799, &C), Some(false));
        assert_eq!(fallback_relay(0.5, 1800, &C), Some(true));
    }
    #[test]
    fn duty_is_capped() {
        assert_eq!(fallback_relay(1.0, 1349, &C), Some(true));
        assert_eq!(fallback_relay(1.0, 1350, &C), Some(false));
    }
    #[test]
    fn short_on_time_is_skipped() {
        assert_eq!(fallback_relay(0.09, 0, &C), Some(false)); // 162 s < 180 s
        assert_eq!(fallback_relay(0.11, 0, &C), Some(true)); // 198 s
        assert_eq!(fallback_relay(0.0, 0, &C), Some(false));
        assert_eq!(fallback_relay(f32::NAN, 0, &C), Some(false));
        assert_eq!(fallback_relay(-1.0, 0, &C), Some(false));
    }
    #[test]
    fn expires_after_six_hours() {
        assert!(fallback_relay(0.5, 21_600, &C).is_some());
        assert_eq!(fallback_relay(0.5, 21_601, &C), None);
    }
    #[test]
    fn controller_without_history_never_heats() {
        let mut c = FallbackController::new();
        assert_eq!(c.on_unavailable(1000, &C), FallbackDecision::NoHistory);
    }
    #[test]
    fn controller_with_minutes_of_history_stays_off() {
        let mut c = FallbackController::new();
        for _ in 0..50 {
            c.on_reading(true, 6);
        } // 5 min of heating after boot
        assert_eq!(c.on_unavailable(1000, &C), FallbackDecision::Relay(false));
    }
    #[test]
    fn controller_cycles_then_expires_then_recovers() {
        let mut c = FallbackController::new();
        for i in 0..12_000 {
            c.on_reading((i / 100) % 2 == 0, 6);
        }
        assert_eq!(c.on_unavailable(50_000, &C), FallbackDecision::Relay(true)); // elapsed 0
        assert_eq!(c.on_unavailable(51_700, &C), FallbackDecision::Relay(false)); // elapsed 1700
        assert_eq!(c.on_unavailable(71_601, &C), FallbackDecision::Expired); // elapsed 21_601
        c.on_reading(false, 6);
        assert_eq!(c.on_unavailable(80_000, &C), FallbackDecision::Relay(true));
        // a new outage restarts the clock
    }
    #[test]
    fn duty_is_frozen_during_fallback() {
        let mut c = FallbackController::new();
        for _ in 0..1200 {
            c.on_reading(true, 6);
        }
        let d = c.duty();
        for s in 0..100 {
            c.on_unavailable(10_000 + s * 6, &C);
        }
        assert_eq!(c.duty(), d);
    }
    #[test]
    fn status_strings() {
        assert_eq!(SensorStatus::Ok.as_str(), "ok");
        assert_eq!(SensorStatus::Fallback.as_str(), "fallback");
        assert_eq!(SensorStatus::Lost.as_str(), "lost");
    }
}
