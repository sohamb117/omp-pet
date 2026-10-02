//! Sleep uses a single deadline, never a polling loop. Idle heartbeats do not reset it.
use std::time::{Duration, Instant};

pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const MIN_SLEEP_FRAME_MS: u64 = 1000;

#[derive(Default)]
pub struct IdleSleep {
    idle_since: Option<Instant>,
}
impl IdleSleep {
    /// None means work needs attention; zero means the pet may sleep now.
    pub fn remaining(&mut self, needs_attention: bool, now: Instant) -> Option<Duration> {
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
    fn sleeps_after_five_minutes_despite_idle_heartbeats() {
        let now = Instant::now();
        let mut state = IdleSleep::default();
        assert_eq!(state.remaining(false, now), Some(IDLE_TIMEOUT));
        for secs in (5..300).step_by(5) {
            assert_eq!(
                state.remaining(false, now + Duration::from_secs(secs)),
                Some(Duration::from_secs(300 - secs))
            );
        }
        assert_eq!(
            state.remaining(false, now + IDLE_TIMEOUT),
            Some(Duration::ZERO)
        );
        assert_eq!(
            state.remaining(false, now + IDLE_TIMEOUT * 2),
            Some(Duration::ZERO)
        );
    }
    #[test]
    fn work_wakes_immediately_and_restarts_the_full_idle_delay() {
        let now = Instant::now();
        let mut state = IdleSleep::default();
        state.remaining(false, now);
        assert_eq!(
            state.remaining(false, now + IDLE_TIMEOUT),
            Some(Duration::ZERO)
        );
        assert_eq!(state.remaining(true, now + IDLE_TIMEOUT), None);
        assert_eq!(state.remaining(true, now + IDLE_TIMEOUT * 2), None);
        assert_eq!(
            state.remaining(false, now + IDLE_TIMEOUT * 3),
            Some(IDLE_TIMEOUT)
        );
        assert_eq!(
            state.remaining(true, now + IDLE_TIMEOUT * 3 + Duration::from_secs(299)),
            None
        );
        assert_eq!(
            state.remaining(false, now + IDLE_TIMEOUT * 4),
            Some(IDLE_TIMEOUT)
        );
    }
}
