//! Per-stream rate cap.

use std::time::{Duration, Instant};

pub struct RateLimiter {
    period: Option<Duration>,
    last: Option<Instant>,
}

impl RateLimiter {
    /// `hz <= 0` (or non-finite) lets only the first call through.
    pub fn new(hz: f32) -> Self {
        let period = (hz.is_finite() && hz > 0.0).then(|| Duration::from_secs_f32(1.0 / hz));
        RateLimiter { period, last: None }
    }

    pub fn ready(&mut self, now: Instant) -> bool {
        let go = match (self.last, self.period) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(last), Some(p)) => now.checked_duration_since(last).is_some_and(|d| d >= p),
        };
        if go {
            self.last = Some(now);
        }
        go
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rate_limiter_caps() {
        let mut r = RateLimiter::new(20.0);
        let t0 = Instant::now();
        let n = (0..1000u64).filter(|i| r.ready(t0 + Duration::from_millis(*i))).count();
        assert!((19..=21).contains(&n), "{n}");
    }

    #[test]
    fn zero_rate_never_fires_after_first_and_clock_going_back_is_safe() {
        let mut r = RateLimiter::new(0.0);
        let t0 = Instant::now();
        assert!(r.ready(t0));
        assert!(!r.ready(t0 + Duration::from_secs(1000)));
        let mut r = RateLimiter::new(10.0);
        let t1 = t0 + Duration::from_secs(5);
        assert!(r.ready(t1));
        assert!(!r.ready(t0));
    }
}
