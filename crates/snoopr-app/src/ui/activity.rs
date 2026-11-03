//! The activity line: 2 px at the bottom of the top bar. A segment sweeps
//! while scanning or emptying the bin, a bar fills while deleting.

use crate::session::Activity;
use crate::theme::ACCENT;
use egui::{Rect, Ui, pos2};
use std::time::Instant;

/// Seconds for the segment to cross the line.
pub const SWEEP_PERIOD: f64 = 1.2;
pub const SEGMENT: f32 = 0.3;
pub const FADE: f32 = 0.3;
/// Height of the line, in px.
const HEIGHT: f32 = 2.0;
/// Time constant of the smoothed fill, in seconds.
const TAU: f32 = 0.15;

/// Indeterminate segment `[x0, x1]` (fractions of the width) at `t` seconds:
/// travels from `-SEGMENT` to 1.
pub fn sweep(t: f64) -> (f32, f32) {
    let phase = (t.rem_euclid(SWEEP_PERIOD) / SWEEP_PERIOD) as f32;
    let x0 = -SEGMENT + (1.0 + SEGMENT) * phase;
    (x0, x0 + SEGMENT)
}

/// Moves `current` towards `target` with a time constant of 0.15 s.
pub fn approach(current: f32, target: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-dt / TAU).exp())
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Sweep,
    Fill,
}

pub struct ActivityLine {
    enabled: bool,
    origin: Instant,
    last_frame: Option<Instant>,
    /// Opacity, 0..=1.
    level: f32,
    /// Shown share of the fill, 0..=1.
    shown: f32,
    mode: Mode,
}

impl ActivityLine {
    pub fn new(enabled: bool) -> Self {
        ActivityLine {
            enabled,
            origin: Instant::now(),
            last_frame: None,
            level: 0.0,
            shown: 0.0,
            mode: Mode::Sweep,
        }
    }

    /// Paints the line over the bottom edge of `under`; true while animating.
    pub fn paint(&mut self, ui: &Ui, under: Rect, a: Activity, now: Instant) -> bool {
        let dt = self
            .last_frame
            .map_or(0.0, |l| now.saturating_duration_since(l).as_secs_f32())
            .min(0.1);
        self.last_frame = Some(now);
        let step = if self.enabled { dt / FADE } else { 1.0 };
        let mut target_fill = self.shown;
        match a {
            Activity::Idle => {
                self.level = (self.level - step).max(0.0);
                if self.level == 0.0 {
                    self.shown = 0.0;
                }
            }
            Activity::Scanning | Activity::EmptyingBin => {
                self.mode = Mode::Sweep;
                self.level = (self.level + step).min(1.0);
            }
            Activity::Deleting { done, total } => {
                self.mode = Mode::Fill;
                self.level = (self.level + step).min(1.0);
                target_fill = if total == 0 {
                    0.0
                } else {
                    (done as f32 / total as f32).clamp(0.0, 1.0)
                };
                self.shown = if self.enabled {
                    approach(self.shown, target_fill, dt)
                } else {
                    target_fill
                };
            }
        }
        if self.level <= 0.0 {
            return false;
        }
        let line = Rect::from_min_max(
            pos2(under.left(), under.bottom() - HEIGHT),
            pos2(under.right(), under.bottom()),
        );
        let w = line.width();
        let (x0, x1) = match self.mode {
            // Without motion the sweep is a still, full-width line.
            Mode::Sweep if !self.enabled => (0.0, 1.0),
            Mode::Sweep => sweep(now.saturating_duration_since(self.origin).as_secs_f64()),
            Mode::Fill => (0.0, self.shown),
        };
        let seg = Rect::from_min_max(
            pos2(line.left() + w * x0.clamp(0.0, 1.0), line.top()),
            pos2(line.left() + w * x1.clamp(0.0, 1.0), line.bottom()),
        );
        let alpha = if self.enabled || self.mode == Mode::Fill {
            self.level
        } else {
            self.level * 0.5
        };
        if seg.width() > 0.0 {
            ui.painter_at(line)
                .rect_filled(seg, 0.0, ACCENT.gamma_multiply(alpha));
        }
        if !self.enabled {
            return false;
        }
        match a {
            Activity::Deleting { .. } => {
                self.level < 1.0 || (self.shown - target_fill).abs() > 0.001
            }
            Activity::Idle | Activity::Scanning | Activity::EmptyingBin => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn sweep_start_and_period() {
        assert_eq!(sweep(0.0), (-0.3, 0.0));
        assert_eq!(sweep(1.2), sweep(0.0));
    }

    #[test]
    fn sweep_midway() {
        let (x0, x1) = sweep(0.6);
        assert!(close(x0, 0.35) && close(x1, 0.65), "{x0} {x1}");
    }

    #[test]
    fn approach_time_constant() {
        assert!((approach(0.0, 1.0, 0.15) - 0.632).abs() < 1e-3);
        assert_eq!(approach(0.4, 0.4, 0.05), 0.4);
    }
}
