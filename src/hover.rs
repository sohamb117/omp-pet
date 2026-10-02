//! A hover reveal is temporary; explicitly showing a pet never arms re-tucking.
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct HoverReveal {
    active: bool,
    left_at: Option<Instant>,
}
impl HoverReveal {
    pub fn start(&mut self) {
        self.active = true;
        self.left_at = None;
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn should_tuck(&mut self, inside: bool, pinned: bool, now: Instant) -> bool {
        if !self.active || inside || pinned {
            self.left_at = None;
            return false;
        }
        // Allow crossing the gap between the sprite and task card without flicker.
        now.duration_since(*self.left_at.get_or_insert(now)) >= Duration::from_millis(350)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_reveal_returns_to_tucked_after_leaving() {
        let now = Instant::now();
        let mut peek = HoverReveal::default();
        assert!(!peek.should_tuck(false, false, now));
        peek.start();
        assert!(!peek.should_tuck(true, false, now));
        assert!(!peek.should_tuck(false, false, now));
        assert!(!peek.should_tuck(false, false, now + Duration::from_millis(349)));
        assert!(peek.should_tuck(false, false, now + Duration::from_millis(350)));
    }
    #[test]
    fn reentry_pin_and_explicit_show_cancel_pending_retuck() {
        let now = Instant::now();
        let mut peek = HoverReveal::default();
        peek.start();
        peek.should_tuck(false, false, now);
        assert!(!peek.should_tuck(true, false, now + Duration::from_millis(200)));
        assert!(!peek.should_tuck(false, false, now + Duration::from_millis(400)));
        assert!(!peek.should_tuck(false, true, now + Duration::from_secs(1)));
        assert!(!peek.should_tuck(false, false, now + Duration::from_secs(2)));
        assert!(peek.should_tuck(false, false, now + Duration::from_secs(3)));
        peek = HoverReveal::default(); // Explicit show, drag, resize, or tuck.
        assert!(!peek.should_tuck(false, false, now + Duration::from_secs(10)));
    }
}
