use serde::{Deserialize, Serialize};

/// Default technical watchdog limit per period (75 minutes = 4,500,000 ms).
/// This is NOT a business added-time cap; it only protects against runaway infinite loops.
pub const DEFAULT_WATCHDOG_MAX_PERIOD_MS: i64 = 75 * 60 * 1000;
/// Standard 45-minute half regulation duration in milliseconds.
pub const DEFAULT_HALF_REGULATION_MS: i64 = 45 * 60 * 1000;
/// Standard 15-minute extra-time half regulation duration in milliseconds.
pub const DEFAULT_EXTRA_TIME_HALF_REGULATION_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Period {
    NotStarted = 0,
    FirstHalf = 1,
    SecondHalf = 2,
    ExtraTimeFirst = 3,
    ExtraTimeSecond = 4,
    Shootout = 5,
    FullTime = 6,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub game_second: i32,
    pub period: Period,
    pub period_started_at_second: i32,
    pub stoppage_accumulator_seconds: i32,
    pub added_time_seconds: i32,
    pub added_time_extension_seconds: i32,
    pub added_time_cap_seconds: i32,
    pub added_time_locked: bool,
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            game_second: 0,
            period: Period::NotStarted,
            period_started_at_second: 0,
            stoppage_accumulator_seconds: 0,
            added_time_seconds: 0,
            added_time_extension_seconds: 0,
            added_time_cap_seconds: 0,
            added_time_locked: false,
        }
    }
}

impl ClockState {
    pub fn set_game_second(&mut self, value: i32) -> Result<(), &'static str> {
        if value < self.game_second {
            return Err("match clock cannot move backwards");
        }
        self.game_second = value;
        Ok(())
    }

    pub fn start_period(&mut self, value: Period) {
        self.period = value;
        self.period_started_at_second = self.game_second;
        self.stoppage_accumulator_seconds = 0;
        self.added_time_seconds = 0;
        self.added_time_extension_seconds = 0;
        self.added_time_cap_seconds = 0;
        self.added_time_locked = false;
    }

    pub fn set_period(&mut self, value: Period) {
        self.period = value;
    }

    pub fn add_stoppage_seconds(&mut self, value: i32) {
        let add = value.max(0);
        if add == 0 {
            return;
        }
        if !self.added_time_locked {
            self.stoppage_accumulator_seconds =
                self.stoppage_accumulator_seconds.saturating_add(add);
        } else if self.added_time_cap_seconds > 0 {
            self.added_time_extension_seconds = (self.added_time_cap_seconds
                - self.added_time_seconds)
                .max(0)
                .min(self.added_time_extension_seconds.saturating_add(add));
        }
    }

    pub fn lock_added_time(&mut self, cap_seconds: i32) -> i32 {
        if !self.added_time_locked {
            self.added_time_cap_seconds = cap_seconds.max(0);
            let rounded = ((self.stoppage_accumulator_seconds + 14) / 15) * 15;
            self.added_time_seconds = self.added_time_cap_seconds.min(rounded);
            self.added_time_locked = true;
        }
        self.added_time_seconds
    }

    pub fn effective_added_time(&self) -> i32 {
        self.added_time_seconds + self.added_time_extension_seconds
    }

    /// Millisecond helper view of `effective_added_time()` for V3/V4 interop.
    pub fn effective_added_ms(&self) -> i64 {
        (self.effective_added_time() as i64).saturating_mul(1000)
    }

    pub fn is_full_time(&self) -> bool {
        self.period == Period::FullTime
    }
}

/// DES v1.4 millisecond-precision monotonic match clock (`i64` ms).
///
/// Key invariants:
/// - Clock never moves backwards (`advance_ms` rejects `dt_ms < 0`).
/// - No hard business cap on added time (e.g. 4–6 minutes); `effective_added_ms()`
///   always equals `announced_added_ms + additional_added_ms`.
/// - Only a technical `watchdog_max_period_ms` guard (default 75 min/period) returns
///   `Err` to surface runaway simulation loops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesClockState {
    pub sim_time_ms: i64,
    pub period: Period,
    pub period_started_at_ms: i64,
    pub period_elapsed_ms: i64,
    pub period_regulation_ms: i64,
    pub ball_in_play_ms: i64,
    pub ball_out_of_play_ms: i64,
    pub compensable_lost_ms: i64,
    pub announced_added_ms: i64,
    pub additional_added_ms: i64,
    pub added_time_locked: bool,
    pub watchdog_max_period_ms: i64,
}

impl Default for DesClockState {
    fn default() -> Self {
        Self {
            sim_time_ms: 0,
            period: Period::NotStarted,
            period_started_at_ms: 0,
            period_elapsed_ms: 0,
            period_regulation_ms: DEFAULT_HALF_REGULATION_MS,
            ball_in_play_ms: 0,
            ball_out_of_play_ms: 0,
            compensable_lost_ms: 0,
            announced_added_ms: 0,
            additional_added_ms: 0,
            added_time_locked: false,
            watchdog_max_period_ms: DEFAULT_WATCHDOG_MAX_PERIOD_MS,
        }
    }
}

impl DesClockState {
    #[inline]
    pub fn effective_added_ms(&self) -> i64 {
        self.announced_added_ms
            .saturating_add(self.additional_added_ms)
    }

    #[inline]
    pub fn period_target_ms(&self) -> i64 {
        self.period_regulation_ms
            .saturating_add(self.effective_added_ms())
    }

    #[inline]
    pub fn game_second(&self) -> i32 {
        (self.sim_time_ms / 1000) as i32
    }

    #[inline]
    pub fn period_elapsed_second(&self) -> i32 {
        (self.period_elapsed_ms / 1000) as i32
    }

    #[inline]
    pub fn is_full_time(&self) -> bool {
        self.period == Period::FullTime
    }

    /// Advances the DES clock by `dt_ms` milliseconds.
    /// Rejects negative `dt_ms` and enforces the technical `watchdog_max_period_ms` guard.
    pub fn advance_ms(&mut self, dt_ms: i64, is_ball_in_play: bool) -> Result<(), &'static str> {
        if dt_ms < 0 {
            return Err("DES match clock cannot move backwards");
        }
        let next_sim_time_ms = self
            .sim_time_ms
            .checked_add(dt_ms)
            .ok_or("DES sim_time_ms overflow")?;
        let next_period_elapsed_ms = self
            .period_elapsed_ms
            .checked_add(dt_ms)
            .ok_or("DES period_elapsed_ms overflow")?;

        if self.watchdog_max_period_ms > 0 && next_period_elapsed_ms > self.watchdog_max_period_ms {
            return Err("DES period elapsed exceeded watchdog_max_period_ms");
        }

        self.sim_time_ms = next_sim_time_ms;
        self.period_elapsed_ms = next_period_elapsed_ms;
        if is_ball_in_play {
            self.ball_in_play_ms = self.ball_in_play_ms.saturating_add(dt_ms);
        } else {
            self.ball_out_of_play_ms = self.ball_out_of_play_ms.saturating_add(dt_ms);
        }
        Ok(())
    }

    /// Records compensable lost time (stoppages, injuries, celebrations, substitutions).
    /// Before added time is announced, accumulates into `compensable_lost_ms`.
    /// After announcement (`added_time_locked == true`), dynamically extends
    /// `additional_added_ms` with no hard business cap.
    pub fn record_compensable_lost_ms(&mut self, lost_ms: i64) {
        let add = lost_ms.max(0);
        if add == 0 {
            return;
        }
        if !self.added_time_locked {
            self.compensable_lost_ms = self.compensable_lost_ms.saturating_add(add);
        } else {
            self.additional_added_ms = self.additional_added_ms.saturating_add(add);
        }
    }

    /// Locks the fourth-official announced added time for the current period.
    /// Subsequent compensable stoppages will still dynamically accrue in `additional_added_ms`.
    pub fn lock_announced_added_ms(&mut self, announced_ms: i64) -> i64 {
        if !self.added_time_locked {
            self.announced_added_ms = announced_ms.max(0);
            self.added_time_locked = true;
        }
        self.announced_added_ms
    }

    /// Starts a new match period, resetting per-period elapsed and added-time counters
    /// while preserving cumulative `sim_time_ms`, `ball_in_play_ms`, `ball_out_of_play_ms`,
    /// and `watchdog_max_period_ms`.
    pub fn begin_period(&mut self, period: Period, regulation_ms: i64) {
        self.period = period;
        self.period_started_at_ms = self.sim_time_ms;
        self.period_elapsed_ms = 0;
        self.period_regulation_ms = regulation_ms.max(0);
        self.compensable_lost_ms = 0;
        self.announced_added_ms = 0;
        self.additional_added_ms = 0;
        self.added_time_locked = false;
    }

    /// Projects this `DesClockState` onto a legacy V3-compatible `ClockState` snapshot.
    pub fn to_legacy_clock_state(&self) -> ClockState {
        ClockState {
            game_second: self.game_second(),
            period: self.period,
            period_started_at_second: (self.period_started_at_ms / 1000) as i32,
            stoppage_accumulator_seconds: (self.compensable_lost_ms / 1000) as i32,
            added_time_seconds: (self.announced_added_ms / 1000) as i32,
            added_time_extension_seconds: (self.additional_added_ms / 1000) as i32,
            added_time_cap_seconds: 0,
            added_time_locked: self.added_time_locked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_time_lock_and_extension_java_parity() {
        let mut c = ClockState::default();
        c.start_period(Period::FirstHalf);
        c.set_game_second(2698).unwrap();
        c.add_stoppage_seconds(19);
        c.add_stoppage_seconds(37);
        assert_eq!(c.lock_added_time(360), 60);
        c.add_stoppage_seconds(12);
        assert_eq!(c.effective_added_time(), 72);
        assert_eq!(c.effective_added_ms(), 72_000);
        assert!(c.set_game_second(2697).is_err());
        c.start_period(Period::SecondHalf);
        assert_eq!(c.effective_added_time(), 0);
    }

    #[test]
    fn des_clock_dynamic_added_time_no_hard_cap() {
        let mut clock = DesClockState::default();
        clock.begin_period(Period::SecondHalf, DEFAULT_HALF_REGULATION_MS);
        clock.advance_ms(DEFAULT_HALF_REGULATION_MS, true).unwrap();

        // 11 minutes of compensable stoppages (well above legacy 4-6 min cap)
        clock.record_compensable_lost_ms(11 * 60 * 1000);
        assert_eq!(clock.lock_announced_added_ms(11 * 60 * 1000), 660_000);
        assert_eq!(clock.effective_added_ms(), 660_000);
        assert_eq!(clock.period_target_ms(), (45 + 11) * 60 * 1000);

        // Additional stoppage during added time extends target dynamically
        clock.record_compensable_lost_ms(150_000);
        assert_eq!(clock.additional_added_ms, 150_000);
        assert_eq!(clock.effective_added_ms(), 810_000);
        assert_eq!(clock.period_target_ms(), 45 * 60 * 1000 + 810_000);
    }

    #[test]
    fn des_clock_independent_period_rounding_preserves_fractional_seconds() {
        let mut clock=DesClockState::default();
        clock.begin_period(Period::FirstHalf,DEFAULT_HALF_REGULATION_MS);
        clock.advance_ms(2_700_700,true).unwrap();
        assert_eq!(clock.game_second(),2700);
        assert_eq!(clock.period_elapsed_second(),2700);
        let first=clock.period_elapsed_ms;
        clock.begin_period(Period::SecondHalf,DEFAULT_HALF_REGULATION_MS);
        assert_eq!(clock.sim_time_ms,first);
        assert_eq!(clock.period_elapsed_ms,0);
        assert_eq!(clock.period_elapsed_second(),0);
        clock.advance_ms(2_700_600,false).unwrap();
        assert_eq!(clock.period_elapsed_ms,2_700_600);
        assert_eq!(clock.period_elapsed_second(),2700);
        assert_eq!(clock.sim_time_ms,5_401_300);
        assert_eq!(clock.game_second(),5401);
        assert_eq!(clock.ball_in_play_ms+clock.ball_out_of_play_ms,clock.sim_time_ms);
    }

    #[test]
    fn des_clock_monotonic_and_watchdog_guard() {
        let mut clock = DesClockState::default();
        clock.begin_period(Period::FirstHalf, DEFAULT_HALF_REGULATION_MS);

        assert!(clock.advance_ms(-1, true).is_err());
        assert_eq!(clock.sim_time_ms, 0);

        clock.advance_ms(12_500, true).unwrap();
        clock.advance_ms(3_500, false).unwrap();
        assert_eq!(clock.sim_time_ms, 16_000);
        assert_eq!(clock.period_elapsed_ms, 16_000);
        assert_eq!(clock.ball_in_play_ms, 12_500);
        assert_eq!(clock.ball_out_of_play_ms, 3_500);
        assert_eq!(clock.game_second(), 16);
        assert_eq!(clock.period_elapsed_second(), 16);

        // Exceeding watchdog returns Err without mutating clock
        let err = clock.advance_ms(DEFAULT_WATCHDOG_MAX_PERIOD_MS, true);
        assert!(err.is_err());
        assert_eq!(clock.sim_time_ms, 16_000);

        // Legacy projection matches second-level fields
        let legacy = clock.to_legacy_clock_state();
        assert_eq!(legacy.game_second, 16);
        assert_eq!(legacy.period, Period::FirstHalf);
        assert_eq!(legacy.added_time_cap_seconds, 0);
    }
}
