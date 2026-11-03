//! Shared animation helpers: easing curves, progress of a timed transition and
//! Windows' "Show animations" setting.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Fast start, gentle stop. `t` is clamped to [0, 1].
pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Gentle start and stop. `t` is clamped to [0, 1].
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = 2.0 - 2.0 * t;
        1.0 - u * u * u / 2.0
    }
}

/// Elapsed share (0..=1) of a transition of length `d` started at `start`;
/// 1 for a zero duration.
pub fn progress(start: Instant, now: Instant, d: Duration) -> f32 {
    if d.is_zero() {
        return 1.0;
    }
    let elapsed = now.saturating_duration_since(start).as_secs_f64();
    (elapsed / d.as_secs_f64()).min(1.0) as f32
}

/// Whether the UI moves: Windows' "Show animations in Windows", read once.
#[allow(dead_code)] // used by the painter (task 4)
pub fn motion_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(crate::system::animations_enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ease_out_cubic_values_and_bounds() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(close(ease_out_cubic(0.5), 0.875));
        assert_eq!(ease_out_cubic(-1.0), 0.0);
        assert_eq!(ease_out_cubic(2.0), 1.0);
    }

    #[test]
    fn ease_in_out_cubic_values() {
        assert!(close(ease_in_out_cubic(0.5), 0.5));
        assert!(close(ease_in_out_cubic(0.25), 0.0625));
        assert_eq!(ease_in_out_cubic(0.0), 0.0);
        assert_eq!(ease_in_out_cubic(1.0), 1.0);
    }

    #[test]
    fn progress_over_a_duration() {
        let t0 = Instant::now();
        let d = Duration::from_millis(200);
        assert!(close(progress(t0, t0 + d / 2, d), 0.5));
        assert_eq!(progress(t0, t0, Duration::ZERO), 1.0);
        assert_eq!(progress(t0, t0 + d * 3, d), 1.0);
        assert_eq!(progress(t0 + d, t0, d), 0.0);
    }
}
