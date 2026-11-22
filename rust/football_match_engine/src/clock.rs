#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
            self.stoppage_accumulator_seconds = self.stoppage_accumulator_seconds.saturating_add(add);
        } else if self.added_time_cap_seconds > 0 {
            self.added_time_extension_seconds = (self.added_time_cap_seconds - self.added_time_seconds)
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

    pub fn is_full_time(&self) -> bool {
        self.period == Period::FullTime
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
        assert!(c.set_game_second(2697).is_err());
        c.start_period(Period::SecondHalf);
        assert_eq!(c.effective_added_time(), 0);
    }
}
