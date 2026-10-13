//! Per rule cool down. Time is the `at` of each event, never a wall clock, so
//! tests with fixed timestamps are deterministic.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

pub struct CoolDown {
    window: Duration,
    /// The `at` of the last event let through, per rule id.
    last: HashMap<String, SystemTime>,
    /// Events dropped since the last one let through, per rule id.
    dropped: HashMap<String, u32>,
}

impl CoolDown {
    pub fn new(window: Duration) -> CoolDown {
        CoolDown {
            window,
            last: HashMap::new(),
            dropped: HashMap::new(),
        }
    }

    pub fn set_window(&mut self, window: Duration) {
        self.window = window;
    }

    /// `Some(coalesced)` when the event is let through: `coalesced` is how many
    /// events for this rule were dropped since the last one let through, and the
    /// count starts again from zero. `None` when the event is inside the window
    /// that follows the last event let through; it is then counted.
    ///
    /// An event dated before the last one let through is let through.
    pub fn admit(&mut self, rule: &str, at: SystemTime) -> Option<u32> {
        if let Some(last) = self.last.get(rule) {
            let inside = at
                .duration_since(*last)
                .map(|gap| gap < self.window)
                .unwrap_or(false);
            if inside {
                *self.dropped.entry(rule.to_string()).or_insert(0) += 1;
                return None;
            }
        }
        self.last.insert(rule.to_string(), at);
        Some(self.dropped.remove(rule).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn t(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn first_event_is_let_through_with_zero() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        assert_eq!(c.admit("r", t(100)), Some(0));
    }

    #[test]
    fn events_inside_the_window_are_dropped_and_counted() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        assert_eq!(c.admit("r", t(100)), Some(0));
        assert_eq!(c.admit("r", t(101)), None);
        assert_eq!(c.admit("r", t(101)), None);
        // Exactly the window after the last event let through is outside it.
        assert_eq!(c.admit("r", t(102)), Some(2));
        // The count was reset.
        assert_eq!(c.admit("r", t(104)), Some(0));
    }

    #[test]
    fn the_window_runs_from_the_last_event_let_through() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        assert_eq!(c.admit("r", t(100)), Some(0));
        assert_eq!(c.admit("r", t(101)), None);
        // 101 was dropped, so it did not move the window: 103 is 3 s after 100.
        assert_eq!(c.admit("r", t(103)), Some(1));
    }

    #[test]
    fn rules_are_independent() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        assert_eq!(c.admit("a", t(100)), Some(0));
        assert_eq!(c.admit("b", t(100)), Some(0));
        assert_eq!(c.admit("a", t(101)), None);
        assert_eq!(c.admit("b", t(103)), Some(0));
    }

    #[test]
    fn a_zero_window_lets_everything_through() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        c.set_window(Duration::ZERO);
        assert_eq!(c.admit("r", t(100)), Some(0));
        assert_eq!(c.admit("r", t(100)), Some(0));
    }

    #[test]
    fn an_event_dated_before_the_last_is_let_through() {
        let mut c = CoolDown::new(Duration::from_secs(2));
        assert_eq!(c.admit("r", t(100)), Some(0));
        assert_eq!(c.admit("r", t(90)), Some(0));
    }
}
