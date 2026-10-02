//! Disconnection sleeps immediately; connected idle uses one deadline, never polling.
use std::time::{Duration, Instant};

pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
pub const MIN_SLEEP_FRAME_MS: u64 = 1000;

#[derive(Default)]
pub struct IdleSleep {
    idle_since: Option<Instant>,
}
impl IdleSleep {
    /// None means awake; zero means sleep now; a positive duration arms one timer.
    pub fn remaining(
        &mut self,
        connected: bool,
        needs_attention: bool,
        now: Instant,
    ) -> Option<Duration> {
        if !connected {
            self.idle_since = None;
            return Some(Duration::ZERO);
        }
        if needs_attention {
            self.idle_since = None;
            return None;
        }
        let since = *self.idle_since.get_or_insert(now);
        Some(IDLE_TIMEOUT.saturating_sub(now.duration_since(since)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_idle_sleeps_at_one_minute_despite_heartbeats() {
        let now = Instant::now();
        let mut state = IdleSleep::default();
        assert_eq!(state.remaining(true, false, now), Some(IDLE_TIMEOUT));
        for secs in 1..60 {
            assert_eq!(
                state.remaining(true, false, now + Duration::from_secs(secs)),
                Some(Duration::from_secs(60 - secs))
            );
        }
        assert_eq!(
            state.remaining(true, false, now + IDLE_TIMEOUT),
            Some(Duration::ZERO)
        );
    }
    #[test]
    fn disconnected_sleeps_immediately_and_reconnect_gets_a_fresh_minute() {
        let now = Instant::now();
        let mut state = IdleSleep::default();
        assert_eq!(state.remaining(false, false, now), Some(Duration::ZERO));
        assert_eq!(state.remaining(true, false, now), Some(IDLE_TIMEOUT));
        assert_eq!(
            state.remaining(false, false, now + Duration::from_secs(10)),
            Some(Duration::ZERO)
        );
        assert_eq!(
            state.remaining(true, false, now + Duration::from_secs(20)),
            Some(IDLE_TIMEOUT)
        );
        assert_eq!(
            state.remaining(true, true, now + Duration::from_secs(21)),
            None
        );
        // A disconnected transport sleeps even if the last snapshot was working.
        assert_eq!(
            state.remaining(false, true, now + Duration::from_secs(22)),
            Some(Duration::ZERO)
        );
    }
    #[test]
    fn work_wakes_immediately_and_restarts_idle_deadline() {
        let now = Instant::now();
        let mut state = IdleSleep::default();
        state.remaining(true, false, now);
        assert_eq!(
            state.remaining(true, false, now + IDLE_TIMEOUT),
            Some(Duration::ZERO)
        );
        assert_eq!(state.remaining(true, true, now + IDLE_TIMEOUT), None);
        assert_eq!(
            state.remaining(true, false, now + IDLE_TIMEOUT),
            Some(IDLE_TIMEOUT)
        );
    }
}
